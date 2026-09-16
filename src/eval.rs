// Evaluates a parsed formula to a scalar value.
//
// This covers the constant-expression subset: literals, arithmetic,
// comparison, concatenation, and the unary operators, following the type
// coercion and error-propagation rules spreadsheets use (text coerces to
// number where possible, booleans count as 1/0 in arithmetic, an error
// operand short-circuits the whole expression). Cell references, defined
// names, function calls, ranges, and the reference operators all depend on
// a workbook to look values up in, which doesn't exist yet, so those report
// Unsupported rather than guessing at a value.

use crate::parser::{BinaryOp, Expr, UnaryOp};
use crate::printer::{format_number, push_json_string};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
    Boolean(bool),
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Unsupported(pub &'static str);

impl Unsupported {
    pub fn message(&self) -> String {
        format!("evaluation of {} is not supported yet", self.0)
    }
}

pub fn eval(expr: &Expr) -> Result<Value, Unsupported> {
    match expr {
        Expr::Number(n) => Ok(Value::Number(*n)),
        Expr::Text(s) => Ok(Value::Text(s.clone())),
        Expr::Boolean(b) => Ok(Value::Boolean(*b)),
        Expr::Error(e) => Ok(Value::Error(e.clone())),
        Expr::Unary(op, inner) => Ok(eval_unary(op, eval(inner)?)),
        Expr::Binary(op, left, right) => Ok(eval_binary(op, eval(left)?, eval(right)?)),
        Expr::Name(_) => Err(Unsupported("defined names")),
        Expr::Reference(_) => Err(Unsupported("cell references")),
        Expr::Range(..) => Err(Unsupported("ranges")),
        Expr::Intersect(..) => Err(Unsupported("the intersect operator")),
        Expr::Union(..) => Err(Unsupported("the union operator")),
        Expr::Array(..) => Err(Unsupported("array literals")),
        Expr::Call(..) => Err(Unsupported("function calls")),
    }
}

fn eval_unary(op: &UnaryOp, v: Value) -> Value {
    if let Value::Error(_) = v {
        return v;
    }
    let n = match coerce_number(&v) {
        Ok(n) => n,
        Err(e) => return e,
    };
    match op {
        UnaryOp::Neg => Value::Number(-n),
        UnaryOp::Pos => Value::Number(n),
        UnaryOp::Percent => Value::Number(n / 100.0),
    }
}

fn eval_binary(op: &BinaryOp, l: Value, r: Value) -> Value {
    if let Value::Error(_) = l {
        return l;
    }
    if let Value::Error(_) = r {
        return r;
    }
    match op {
        BinaryOp::Add => numeric_op(l, r, |a, b| Ok(a + b)),
        BinaryOp::Sub => numeric_op(l, r, |a, b| Ok(a - b)),
        BinaryOp::Mul => numeric_op(l, r, |a, b| Ok(a * b)),
        BinaryOp::Div => numeric_op(l, r, |a, b| {
            if b == 0.0 {
                Err("#DIV/0!".to_string())
            } else {
                Ok(a / b)
            }
        }),
        BinaryOp::Pow => numeric_op(l, r, |a, b| {
            let result = a.powf(b);
            if result.is_nan() {
                Err("#NUM!".to_string())
            } else {
                Ok(result)
            }
        }),
        BinaryOp::Concat => Value::Text(format!("{}{}", to_text(&l), to_text(&r))),
        BinaryOp::Eq => Value::Boolean(compare(&l, &r) == std::cmp::Ordering::Equal),
        BinaryOp::Ne => Value::Boolean(compare(&l, &r) != std::cmp::Ordering::Equal),
        BinaryOp::Lt => Value::Boolean(compare(&l, &r) == std::cmp::Ordering::Less),
        BinaryOp::Le => Value::Boolean(compare(&l, &r) != std::cmp::Ordering::Greater),
        BinaryOp::Gt => Value::Boolean(compare(&l, &r) == std::cmp::Ordering::Greater),
        BinaryOp::Ge => Value::Boolean(compare(&l, &r) != std::cmp::Ordering::Less),
    }
}

fn numeric_op(l: Value, r: Value, f: impl Fn(f64, f64) -> Result<f64, String>) -> Value {
    let a = match coerce_number(&l) {
        Ok(n) => n,
        Err(e) => return e,
    };
    let b = match coerce_number(&r) {
        Ok(n) => n,
        Err(e) => return e,
    };
    match f(a, b) {
        Ok(n) => Value::Number(n),
        Err(e) => Value::Error(e),
    }
}

// Coercion is deliberately simple: a trimmed string that parses as an f64.
// Real spreadsheets also accept things like "$1,200" or "50%" here; that's
// a case for a proper text-to-number parser later, not this pass.
fn coerce_number(v: &Value) -> Result<f64, Value> {
    match v {
        Value::Number(n) => Ok(*n),
        Value::Boolean(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Value::Text(s) => s.trim().parse::<f64>().map_err(|_| Value::Error("#VALUE!".to_string())),
        Value::Error(e) => Err(Value::Error(e.clone())),
    }
}

fn to_text(v: &Value) -> String {
    match v {
        Value::Number(n) => format_number(*n),
        Value::Text(s) => s.clone(),
        Value::Boolean(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        Value::Error(e) => e.clone(),
    }
}

// Mixed-type comparisons follow spreadsheet sort order rather than failing:
// every number sorts below every piece of text, which sorts below both
// booleans. Same-type comparisons use the obvious ordering, with text
// compared case-insensitively.
fn compare(l: &Value, r: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (l, r) {
        (Value::Number(a), Value::Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (Value::Text(a), Value::Text(b)) => a.to_ascii_uppercase().cmp(&b.to_ascii_uppercase()),
        (Value::Boolean(a), Value::Boolean(b)) => a.cmp(b),
        (Value::Number(_), _) => Ordering::Less,
        (_, Value::Number(_)) => Ordering::Greater,
        (Value::Text(_), Value::Boolean(_)) => Ordering::Less,
        (Value::Boolean(_), Value::Text(_)) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

pub fn to_json(v: &Value) -> String {
    let mut out = String::new();
    match v {
        Value::Number(n) => {
            out.push_str("{\"type\":\"number\",\"value\":");
            out.push_str(&format_number(*n));
            out.push('}');
        }
        Value::Text(s) => {
            out.push_str("{\"type\":\"text\",\"value\":");
            push_json_string(&mut out, s);
            out.push('}');
        }
        Value::Boolean(b) => {
            out.push_str("{\"type\":\"boolean\",\"value\":");
            out.push_str(if *b { "true" } else { "false" });
            out.push('}');
        }
        Value::Error(e) => {
            out.push_str("{\"type\":\"error\",\"value\":");
            push_json_string(&mut out, e);
            out.push('}');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn eval_str(input: &str) -> Value {
        eval(&parse(input).unwrap()).unwrap()
    }

    #[test]
    fn arithmetic_evaluates_in_order() {
        assert_eq!(eval_str("2+3*4"), Value::Number(14.0));
        assert_eq!(eval_str("(2+3)*4"), Value::Number(20.0));
    }

    #[test]
    fn unary_minus_and_percent() {
        assert_eq!(eval_str("-2^2"), Value::Number(4.0));
        assert_eq!(eval_str("50%"), Value::Number(0.5));
    }

    #[test]
    fn division_by_zero_is_a_spreadsheet_error() {
        assert_eq!(eval_str("1/0"), Value::Error("#DIV/0!".to_string()));
    }

    #[test]
    fn fractional_power_of_negative_base_is_num_error() {
        assert_eq!(eval_str("(-1)^0.5"), Value::Error("#NUM!".to_string()));
    }

    #[test]
    fn concat_stringifies_numbers_and_booleans() {
        assert_eq!(eval_str("1&\"a\"&TRUE"), Value::Text("1aTRUE".to_string()));
    }

    #[test]
    fn text_that_looks_numeric_coerces_in_arithmetic() {
        assert_eq!(eval_str("\"3\"+\"4\""), Value::Number(7.0));
    }

    #[test]
    fn non_numeric_text_in_arithmetic_is_value_error() {
        assert_eq!(eval_str("\"abc\"+1"), Value::Error("#VALUE!".to_string()));
    }

    #[test]
    fn error_operand_short_circuits_the_whole_expression() {
        assert_eq!(eval_str("1+#REF!*2"), Value::Error("#REF!".to_string()));
    }

    #[test]
    fn comparisons_are_case_insensitive_for_text() {
        assert_eq!(eval_str("\"abc\"=\"ABC\""), Value::Boolean(true));
    }

    #[test]
    fn numbers_sort_below_text_which_sorts_below_booleans() {
        assert_eq!(eval_str("1<\"a\""), Value::Boolean(true));
        assert_eq!(eval_str("\"a\"<TRUE"), Value::Boolean(true));
    }

    #[test]
    fn boolean_coerces_to_one_and_zero_in_arithmetic() {
        assert_eq!(eval_str("TRUE+TRUE"), Value::Number(2.0));
    }

    #[test]
    fn cell_references_are_reported_as_unsupported() {
        let err = eval(&parse("A1+1").unwrap()).unwrap_err();
        assert_eq!(err, Unsupported("cell references"));
    }

    #[test]
    fn function_calls_are_reported_as_unsupported() {
        let err = eval(&parse("SUM(1,2)").unwrap()).unwrap_err();
        assert_eq!(err, Unsupported("function calls"));
    }
}
