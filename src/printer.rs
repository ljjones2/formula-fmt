// Renders a parsed formula back into canonical text, or into JSON.
//
// Precedence numbers here mirror the parser's chain in parser.rs (higher
// number binds tighter). Parentheses are only emitted where the parser
// wouldn't otherwise reconstruct the same tree, so round-tripping a formula
// through parse -> render is idempotent and never changes its meaning.

use crate::parser::{BinaryOp, CellRef, Expr, UnaryOp};

pub fn render(expr: &Expr) -> String {
    match expr {
        Expr::Number(n) => format_number(*n),
        Expr::Text(s) => format!("\"{}\"", s.replace('"', "\"\"")),
        Expr::Boolean(b) => {
            if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Expr::Name(n) => n.clone(),
        Expr::Reference(r) => render_ref(r),
        Expr::Range(a, b) => format!("{}:{}", print_child(a, 8, false), print_child(b, 8, false)),
        Expr::Unary(op, inner) => match op {
            UnaryOp::Neg => format!("-{}", print_child(inner, 7, false)),
            UnaryOp::Pos => format!("+{}", print_child(inner, 7, false)),
            UnaryOp::Percent => format!("{}%", print_child(inner, 6, false)),
        },
        Expr::Binary(op, left, right) => {
            let p = binary_precedence(op);
            format!(
                "{}{}{}",
                print_child(left, p, false),
                binary_symbol(op),
                print_child(right, p, true)
            )
        }
        Expr::Call(name, args) => {
            let rendered: Vec<String> = args.iter().map(render).collect();
            format!("{}({})", name.to_ascii_uppercase(), rendered.join(", "))
        }
    }
}

fn print_child(expr: &Expr, parent_prec: u8, right_side: bool) -> String {
    let text = render(expr);
    let child_prec = precedence(expr);
    let needs_parens = if right_side { child_prec <= parent_prec } else { child_prec < parent_prec };
    if needs_parens {
        format!("({})", text)
    } else {
        text
    }
}

fn binary_precedence(op: &BinaryOp) -> u8 {
    match op {
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => 1,
        BinaryOp::Concat => 2,
        BinaryOp::Add | BinaryOp::Sub => 3,
        BinaryOp::Mul | BinaryOp::Div => 4,
        BinaryOp::Pow => 5,
    }
}

fn precedence(expr: &Expr) -> u8 {
    match expr {
        Expr::Binary(op, ..) => binary_precedence(op),
        Expr::Unary(UnaryOp::Percent, _) => 6,
        Expr::Unary(UnaryOp::Neg, _) | Expr::Unary(UnaryOp::Pos, _) => 7,
        Expr::Range(..) => 8,
        _ => 9,
    }
}

fn binary_symbol(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Pow => "^",
        BinaryOp::Concat => "&",
        BinaryOp::Eq => "=",
        BinaryOp::Ne => "<>",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
    }
}

fn binary_op_name(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "add",
        BinaryOp::Sub => "sub",
        BinaryOp::Mul => "mul",
        BinaryOp::Div => "div",
        BinaryOp::Pow => "pow",
        BinaryOp::Concat => "concat",
        BinaryOp::Eq => "eq",
        BinaryOp::Ne => "ne",
        BinaryOp::Lt => "lt",
        BinaryOp::Le => "le",
        BinaryOp::Gt => "gt",
        BinaryOp::Ge => "ge",
    }
}

fn render_ref(r: &CellRef) -> String {
    let mut out = String::new();
    if let Some(sheet) = &r.sheet {
        out.push_str(sheet);
        out.push('!');
    }
    if r.col_absolute {
        out.push('$');
    }
    out.push_str(&r.column);
    if r.row_absolute {
        out.push('$');
    }
    out.push_str(&r.row.to_string());
    out
}

fn format_number(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

pub fn to_json(expr: &Expr) -> String {
    let mut out = String::new();
    write_json(expr, &mut out);
    out
}

fn write_json(expr: &Expr, out: &mut String) {
    match expr {
        Expr::Number(n) => {
            out.push_str("{\"type\":\"number\",\"value\":");
            out.push_str(&format_number(*n));
            out.push('}');
        }
        Expr::Text(s) => {
            out.push_str("{\"type\":\"text\",\"value\":");
            push_json_string(out, s);
            out.push('}');
        }
        Expr::Boolean(b) => {
            out.push_str("{\"type\":\"boolean\",\"value\":");
            out.push_str(if *b { "true" } else { "false" });
            out.push('}');
        }
        Expr::Name(n) => {
            out.push_str("{\"type\":\"name\",\"value\":");
            push_json_string(out, n);
            out.push('}');
        }
        Expr::Reference(r) => {
            out.push_str("{\"type\":\"reference\",\"sheet\":");
            match &r.sheet {
                Some(s) => push_json_string(out, s),
                None => out.push_str("null"),
            }
            out.push_str(",\"column\":");
            push_json_string(out, &r.column);
            out.push_str(",\"row\":");
            out.push_str(&r.row.to_string());
            out.push_str(",\"colAbsolute\":");
            out.push_str(if r.col_absolute { "true" } else { "false" });
            out.push_str(",\"rowAbsolute\":");
            out.push_str(if r.row_absolute { "true" } else { "false" });
            out.push('}');
        }
        Expr::Range(a, b) => {
            out.push_str("{\"type\":\"range\",\"start\":");
            write_json(a, out);
            out.push_str(",\"end\":");
            write_json(b, out);
            out.push('}');
        }
        Expr::Unary(op, inner) => {
            out.push_str("{\"type\":\"unary\",\"op\":\"");
            out.push_str(match op {
                UnaryOp::Neg => "neg",
                UnaryOp::Pos => "pos",
                UnaryOp::Percent => "percent",
            });
            out.push_str("\",\"operand\":");
            write_json(inner, out);
            out.push('}');
        }
        Expr::Binary(op, left, right) => {
            out.push_str("{\"type\":\"binary\",\"op\":\"");
            out.push_str(binary_op_name(op));
            out.push_str("\",\"left\":");
            write_json(left, out);
            out.push_str(",\"right\":");
            write_json(right, out);
            out.push('}');
        }
        Expr::Call(name, args) => {
            out.push_str("{\"type\":\"call\",\"name\":");
            push_json_string(out, name);
            out.push_str(",\"args\":[");
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json(arg, out);
            }
            out.push_str("]}");
        }
    }
}

pub fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
