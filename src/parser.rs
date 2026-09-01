// Recursive-descent parser producing an AST from formula text.
//
// The chain of parse_* functions below is ordered from lowest to highest
// precedence, matching the operator precedence table Microsoft documents for
// Excel: comparisons bind loosest, then concatenation, then + -, then * /,
// then ^, then %, then unary minus/plus, then the range operator ':'.
// The notable surprise (and the reason unary sits above both % and ^ here)
// is that "=-2^2" evaluates to 4 in real spreadsheets, not -4: unary minus
// binds tighter than exponentiation.

use crate::lexer::{Lexer, Token, TokenKind};

#[derive(Debug, Clone, PartialEq)]
pub struct CellRef {
    pub sheet: Option<String>,
    pub col_absolute: bool,
    pub column: String,
    pub row_absolute: bool,
    pub row: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Pos,
    Percent,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Text(String),
    Boolean(bool),
    Name(String),
    Reference(CellRef),
    Range(Box<Expr>, Box<Expr>),
    Union(Vec<Expr>),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub pos: usize,
}

pub fn parse(input: &str) -> Result<Expr, ParseError> {
    let tokens = Lexer::new(input)
        .tokenize()
        .map_err(|e| ParseError { message: e.message, pos: e.pos })?;
    let mut parser = Parser { tokens, index: 0 };
    let expr = parser.parse_expr()?;
    parser.expect_eof()?;
    Ok(expr)
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.index]
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.index].clone();
        if self.index + 1 < self.tokens.len() {
            self.index += 1;
        }
        token
    }

    fn expect_eof(&mut self) -> Result<(), ParseError> {
        if self.peek().kind == TokenKind::Eof {
            Ok(())
        } else {
            Err(ParseError {
                message: format!("unexpected trailing input near '{}'", describe(&self.peek().kind)),
                pos: self.peek().pos,
            })
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        if self.peek().kind == kind {
            Ok(self.advance())
        } else {
            Err(ParseError {
                message: format!(
                    "expected '{}', found '{}'",
                    describe(&kind),
                    describe(&self.peek().kind)
                ),
                pos: self.peek().pos,
            })
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_concat()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Eq => BinaryOp::Eq,
                TokenKind::Ne => BinaryOp::Ne,
                TokenKind::Lt => BinaryOp::Lt,
                TokenKind::Le => BinaryOp::Le,
                TokenKind::Gt => BinaryOp::Gt,
                TokenKind::Ge => BinaryOp::Ge,
                _ => break,
            };
            self.advance();
            let right = self.parse_concat()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_concat(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_additive()?;
        while self.peek().kind == TokenKind::Ampersand {
            self.advance();
            let right = self.parse_additive()?;
            left = Expr::Binary(BinaryOp::Concat, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_pow()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                _ => break,
            };
            self.advance();
            let right = self.parse_pow()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_pow(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_percent()?;
        while self.peek().kind == TokenKind::Caret {
            self.advance();
            let right = self.parse_percent()?;
            left = Expr::Binary(BinaryOp::Pow, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_percent(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_unary()?;
        while self.peek().kind == TokenKind::Percent {
            self.advance();
            expr = Expr::Unary(UnaryOp::Percent, Box::new(expr));
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().kind {
            TokenKind::Minus => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Neg, Box::new(operand)))
            }
            TokenKind::Plus => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Pos, Box::new(operand)))
            }
            _ => self.parse_range(),
        }
    }

    fn parse_range(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_primary()?;
        if self.peek().kind == TokenKind::Colon {
            self.advance();
            let right = self.parse_primary()?;
            Ok(Expr::Range(Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Number(n) => {
                self.advance();
                Ok(Expr::Number(n))
            }
            TokenKind::Text(s) => {
                self.advance();
                Ok(Expr::Text(s))
            }
            TokenKind::LParen => {
                self.advance();
                let first = self.parse_expr()?;
                if self.peek().kind == TokenKind::Comma {
                    // A parenthesized comma list is the union reference operator,
                    // e.g. (A1:A2,B1:B2) - distinct from a function's argument
                    // list, which never routes through here.
                    let mut items = vec![first];
                    while self.peek().kind == TokenKind::Comma {
                        self.advance();
                        items.push(self.parse_expr()?);
                    }
                    self.expect(TokenKind::RParen)?;
                    Ok(Expr::Union(items))
                } else {
                    self.expect(TokenKind::RParen)?;
                    Ok(first)
                }
            }
            TokenKind::Ident(name) => {
                self.advance();
                self.parse_ident_expr(name, token.pos)
            }
            TokenKind::SheetName(name) => {
                self.advance();
                self.expect(TokenKind::Bang)?;
                self.parse_reference_after_bang(name)
            }
            _ => Err(ParseError {
                message: format!("expected a value, found '{}'", describe(&token.kind)),
                pos: token.pos,
            }),
        }
    }

    fn parse_ident_expr(&mut self, name: String, pos: usize) -> Result<Expr, ParseError> {
        if self.peek().kind == TokenKind::LParen {
            self.advance();
            let mut args = Vec::new();
            if self.peek().kind != TokenKind::RParen {
                loop {
                    args.push(self.parse_expr()?);
                    if self.peek().kind == TokenKind::Comma {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RParen)?;
            return Ok(Expr::Call(name, args));
        }

        if self.peek().kind == TokenKind::Bang {
            self.advance();
            return self.parse_reference_after_bang(name);
        }

        let upper = name.to_ascii_uppercase();
        if upper == "TRUE" {
            return Ok(Expr::Boolean(true));
        }
        if upper == "FALSE" {
            return Ok(Expr::Boolean(false));
        }

        match parse_cell_ref(&name) {
            Some(cell) => Ok(Expr::Reference(cell)),
            None if is_valid_name(&name) => Ok(Expr::Name(name)),
            None => Err(ParseError {
                message: format!("'{}' is not a valid reference or name", name),
                pos,
            }),
        }
    }

    // Called just after a '!' has been consumed, with the sheet name (quoted
    // or bare) already parsed. Only a plain cell reference can follow.
    fn parse_reference_after_bang(&mut self, sheet: String) -> Result<Expr, ParseError> {
        let cell_token = self.advance();
        let cell_name = match cell_token.kind {
            TokenKind::Ident(n) => n,
            _ => {
                return Err(ParseError {
                    message: "expected a cell reference after '!'".to_string(),
                    pos: cell_token.pos,
                })
            }
        };
        match parse_cell_ref(&cell_name) {
            Some(mut cell) => {
                cell.sheet = Some(sheet);
                Ok(Expr::Reference(cell))
            }
            None => Err(ParseError {
                message: format!("'{}' is not a valid cell reference", cell_name),
                pos: cell_token.pos,
            }),
        }
    }
}

// Parses text like "A1", "$A$1", "AB12" into a sheet-less CellRef.
// Returns None if the text isn't a plain column-letters + row-digits pattern,
// so callers can fall back to treating it as a defined name.
fn parse_cell_ref(text: &str) -> Option<CellRef> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    let col_absolute = if chars.get(i) == Some(&'$') {
        i += 1;
        true
    } else {
        false
    };

    let col_start = i;
    while matches!(chars.get(i), Some(c) if c.is_ascii_alphabetic()) {
        i += 1;
    }
    if i == col_start {
        return None;
    }
    let column: String = chars[col_start..i].iter().collect::<String>().to_ascii_uppercase();
    if column.len() > 3 {
        return None;
    }

    let row_absolute = if chars.get(i) == Some(&'$') {
        i += 1;
        true
    } else {
        false
    };

    let row_start = i;
    while matches!(chars.get(i), Some(c) if c.is_ascii_digit()) {
        i += 1;
    }
    if i == row_start || i != chars.len() {
        return None;
    }
    let row: u32 = chars[row_start..i].iter().collect::<String>().parse().ok()?;
    if row == 0 {
        return None;
    }

    Some(CellRef { sheet: None, col_absolute, column, row_absolute, row })
}

fn is_valid_name(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printer::render;

    fn canonical(input: &str) -> String {
        format!("={}", render(&parse(input).unwrap()))
    }

    #[test]
    fn unary_minus_binds_tighter_than_pow() {
        // "-2^2" is 4 in Excel, not -4: the minus only grabs its immediate
        // operand, so the tree is (-2)^2, not -(2^2).
        let expr = parse("-2^2").unwrap();
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Pow,
                Box::new(Expr::Unary(UnaryOp::Neg, Box::new(Expr::Number(2.0)))),
                Box::new(Expr::Number(2.0)),
            )
        );
        assert_eq!(canonical("-2^2"), "=-2^2");
    }

    #[test]
    fn pow_right_operand_can_be_unary() {
        // Here the minus is the right operand of ^, so it stays there:
        // 2^-2 is 2^(-2), not (2^-2) folded into something else.
        let expr = parse("2^-2").unwrap();
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Pow,
                Box::new(Expr::Number(2.0)),
                Box::new(Expr::Unary(UnaryOp::Neg, Box::new(Expr::Number(2.0)))),
            )
        );
        assert_eq!(canonical("2^-2"), "=2^-2");
    }

    #[test]
    fn unary_minus_binds_tighter_than_percent() {
        // "-2%" is (-2)%, not -(2%) - unary sits above percent too.
        let expr = parse("-2%").unwrap();
        assert_eq!(
            expr,
            Expr::Unary(
                UnaryOp::Percent,
                Box::new(Expr::Unary(UnaryOp::Neg, Box::new(Expr::Number(2.0)))),
            )
        );
    }

    #[test]
    fn percent_binds_tighter_than_pow() {
        // "2%^3" means (2%)^3: percent grabs 2 before ^ ever sees it.
        let expr = parse("2%^3").unwrap();
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Pow,
                Box::new(Expr::Unary(UnaryOp::Percent, Box::new(Expr::Number(2.0)))),
                Box::new(Expr::Number(3.0)),
            )
        );
        assert_eq!(canonical("2%^3"), "=2%^3");
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        let expr = parse("2+3*4").unwrap();
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Add,
                Box::new(Expr::Number(2.0)),
                Box::new(Expr::Binary(
                    BinaryOp::Mul,
                    Box::new(Expr::Number(3.0)),
                    Box::new(Expr::Number(4.0)),
                )),
            )
        );
        assert_eq!(canonical("2+3*4"), "=2+3*4");
    }

    #[test]
    fn parens_reinstated_when_grouping_overrides_precedence() {
        // The tree here has + nested inside *, so the printer must add back
        // the parentheses or the canonical form would change meaning.
        assert_eq!(canonical("(2+3)*4"), "=(2+3)*4");
    }

    #[test]
    fn range_binds_tighter_than_unary_minus() {
        let expr = parse("-A1:B2").unwrap();
        match expr {
            Expr::Unary(UnaryOp::Neg, inner) => {
                assert!(matches!(*inner, Expr::Range(..)));
            }
            other => panic!("expected a negated range, got {:?}", other),
        }
        assert_eq!(canonical("-A1:B2"), "=-A1:B2");
    }

    #[test]
    fn chained_comparisons_are_left_associative() {
        let expr = parse("1<2=3").unwrap();
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Eq,
                Box::new(Expr::Binary(
                    BinaryOp::Lt,
                    Box::new(Expr::Number(1.0)),
                    Box::new(Expr::Number(2.0)),
                )),
                Box::new(Expr::Number(3.0)),
            )
        );
        assert_eq!(canonical("1<2=3"), "=1<2=3");
    }

    #[test]
    fn concat_binds_looser_than_addition_but_tighter_than_comparison() {
        assert_eq!(canonical("1&2+3=4&5"), "=1&2+3=4&5");
    }

    #[test]
    fn function_and_cell_names_are_canonicalized() {
        assert_eq!(canonical("sum(a1:a10)+total"), "=SUM(A1:A10)+total");
    }

    #[test]
    fn bare_sheet_name_round_trips_without_quotes() {
        assert_eq!(canonical("Sheet1!A1"), "=Sheet1!A1");
    }

    #[test]
    fn quoted_sheet_name_is_required_for_spaces() {
        let expr = parse("'My Sheet'!A1").unwrap();
        match expr {
            Expr::Reference(cell) => assert_eq!(cell.sheet.as_deref(), Some("My Sheet")),
            other => panic!("expected a reference, got {:?}", other),
        }
        assert_eq!(canonical("'My Sheet'!A1"), "='My Sheet'!A1");
    }

    #[test]
    fn quoted_sheet_name_unescapes_doubled_quotes() {
        let expr = parse("'O''Brien''s Sheet'!A1").unwrap();
        match expr {
            Expr::Reference(cell) => assert_eq!(cell.sheet.as_deref(), Some("O'Brien's Sheet")),
            other => panic!("expected a reference, got {:?}", other),
        }
        assert_eq!(canonical("'O''Brien''s Sheet'!A1"), "='O''Brien''s Sheet'!A1");
    }

    #[test]
    fn plain_sheet_name_does_not_pick_up_quotes_it_did_not_have() {
        // a sheet name that happens to be quoted in the input but doesn't
        // need it should print back without the quotes
        assert_eq!(canonical("'Sheet1'!A1"), "=Sheet1!A1");
    }

    #[test]
    fn unterminated_sheet_name_reports_opening_quote_position() {
        let err = parse("'My Sheet!A1").unwrap_err();
        assert_eq!(err.pos, 0);
    }

    #[test]
    fn missing_operand_reports_position_at_end_of_input() {
        let err = parse("1+").unwrap_err();
        assert_eq!(err.pos, 2);
    }

    #[test]
    fn missing_operand_before_operator_reports_operator_position() {
        let err = parse("1+*2").unwrap_err();
        assert_eq!(err.pos, 2);
    }

    #[test]
    fn trailing_input_is_rejected_at_its_own_position() {
        let err = parse("1 2").unwrap_err();
        assert_eq!(err.pos, 2);
    }

    #[test]
    fn parenthesized_comma_list_is_a_union() {
        let expr = parse("(A1:A2,B1:B2)").unwrap();
        assert_eq!(
            expr,
            Expr::Union(vec![
                Expr::Range(
                    Box::new(Expr::Reference(parse_cell_ref("A1").unwrap())),
                    Box::new(Expr::Reference(parse_cell_ref("A2").unwrap())),
                ),
                Expr::Range(
                    Box::new(Expr::Reference(parse_cell_ref("B1").unwrap())),
                    Box::new(Expr::Reference(parse_cell_ref("B2").unwrap())),
                ),
            ])
        );
        assert_eq!(canonical("(A1:A2,B1:B2)"), "=(A1:A2, B1:B2)");
    }

    #[test]
    fn union_can_appear_as_a_function_argument() {
        assert_eq!(canonical("sum((a1:a2,b1:b2))"), "=SUM((A1:A2, B1:B2))");
    }

    #[test]
    fn union_of_more_than_two_members_round_trips() {
        assert_eq!(canonical("(A1,B1,C1)"), "=(A1, B1, C1)");
    }

    #[test]
    fn plain_parens_without_a_comma_stay_a_grouping_not_a_union() {
        let expr = parse("(A1)").unwrap();
        assert_eq!(expr, Expr::Reference(parse_cell_ref("A1").unwrap()));
    }

    #[test]
    fn union_with_trailing_comma_reports_missing_operand() {
        let err = parse("(A1,)").unwrap_err();
        assert_eq!(err.pos, 4);
    }
}

fn describe(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Number(n) => n.to_string(),
        TokenKind::Text(s) => format!("\"{}\"", s),
        TokenKind::Ident(s) => s.clone(),
        TokenKind::SheetName(s) => format!("'{}'", s),
        TokenKind::Plus => "+".to_string(),
        TokenKind::Minus => "-".to_string(),
        TokenKind::Star => "*".to_string(),
        TokenKind::Slash => "/".to_string(),
        TokenKind::Caret => "^".to_string(),
        TokenKind::Ampersand => "&".to_string(),
        TokenKind::Percent => "%".to_string(),
        TokenKind::Eq => "=".to_string(),
        TokenKind::Ne => "<>".to_string(),
        TokenKind::Lt => "<".to_string(),
        TokenKind::Le => "<=".to_string(),
        TokenKind::Gt => ">".to_string(),
        TokenKind::Ge => ">=".to_string(),
        TokenKind::LParen => "(".to_string(),
        TokenKind::RParen => ")".to_string(),
        TokenKind::Comma => ",".to_string(),
        TokenKind::Colon => ":".to_string(),
        TokenKind::Bang => "!".to_string(),
        TokenKind::Eof => "end of formula".to_string(),
    }
}
