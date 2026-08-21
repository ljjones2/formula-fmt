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
                let inner = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(inner)
            }
            TokenKind::Ident(name) => {
                self.advance();
                self.parse_ident_expr(name, token.pos)
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
            return match parse_cell_ref(&cell_name) {
                Some(mut cell) => {
                    cell.sheet = Some(name);
                    Ok(Expr::Reference(cell))
                }
                None => Err(ParseError {
                    message: format!("'{}' is not a valid cell reference", cell_name),
                    pos: cell_token.pos,
                }),
            };
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

fn describe(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Number(n) => n.to_string(),
        TokenKind::Text(s) => format!("\"{}\"", s),
        TokenKind::Ident(s) => s.clone(),
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
