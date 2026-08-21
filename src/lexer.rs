// Tokenizer for spreadsheet formula text (the part after the leading '=').
// Positions are character offsets into the input, used later for error messages.

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Number(f64),
    Text(String),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Ampersand,
    Percent,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    LParen,
    RParen,
    Comma,
    Colon,
    Bang,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub pos: usize,
}

#[derive(Debug)]
pub struct LexError {
    pub message: String,
    pub pos: usize,
}

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Lexer { chars: input.chars().collect(), pos: 0 }
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let is_eof = matches!(token.kind, TokenKind::Eof);
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn next_token(&mut self) -> Result<Token, LexError> {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }

        let start = self.pos;
        let c = match self.peek() {
            None => return Ok(Token { kind: TokenKind::Eof, pos: start }),
            Some(c) => c,
        };

        if c == '"' {
            return self.lex_string(start);
        }
        if c.is_ascii_digit() || (c == '.' && matches!(self.peek_at(1), Some(d) if d.is_ascii_digit()))
        {
            return self.lex_number(start);
        }
        if c.is_alphabetic() || c == '_' || c == '$' {
            return self.lex_ident(start);
        }

        self.pos += 1;
        let kind = match c {
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '^' => TokenKind::Caret,
            '&' => TokenKind::Ampersand,
            '%' => TokenKind::Percent,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            '!' => TokenKind::Bang,
            '=' => TokenKind::Eq,
            '<' => {
                if self.peek() == Some('=') {
                    self.pos += 1;
                    TokenKind::Le
                } else if self.peek() == Some('>') {
                    self.pos += 1;
                    TokenKind::Ne
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                if self.peek() == Some('=') {
                    self.pos += 1;
                    TokenKind::Ge
                } else {
                    TokenKind::Gt
                }
            }
            other => {
                return Err(LexError {
                    message: format!("unexpected character '{}'", other),
                    pos: start,
                })
            }
        };
        Ok(Token { kind, pos: start })
    }

    fn lex_string(&mut self, start: usize) -> Result<Token, LexError> {
        self.pos += 1; // opening quote
        let mut value = String::new();
        loop {
            match self.advance() {
                None => {
                    return Err(LexError {
                        message: "unterminated string literal".to_string(),
                        pos: start,
                    })
                }
                Some('"') => {
                    // a doubled quote is an escaped literal quote inside the string
                    if self.peek() == Some('"') {
                        value.push('"');
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                Some(c) => value.push(c),
            }
        }
        Ok(Token { kind: TokenKind::Text(value), pos: start })
    }

    fn lex_number(&mut self, start: usize) -> Result<Token, LexError> {
        let mut text = String::new();
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            text.push(self.advance().unwrap());
        }
        if self.peek() == Some('.') {
            text.push(self.advance().unwrap());
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                text.push(self.advance().unwrap());
            }
        }
        if matches!(self.peek(), Some('e') | Some('E')) {
            let save = self.pos;
            let mut exp = String::new();
            exp.push(self.advance().unwrap());
            if matches!(self.peek(), Some('+') | Some('-')) {
                exp.push(self.advance().unwrap());
            }
            if matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    exp.push(self.advance().unwrap());
                }
                text.push_str(&exp);
            } else {
                // not actually an exponent (e.g. "1e" with no digits after) - back out
                self.pos = save;
            }
        }
        match text.parse::<f64>() {
            Ok(value) => Ok(Token { kind: TokenKind::Number(value), pos: start }),
            Err(_) => Err(LexError { message: format!("invalid number '{}'", text), pos: start }),
        }
    }

    fn lex_ident(&mut self, start: usize) -> Result<Token, LexError> {
        let mut text = String::new();
        while matches!(self.peek(), Some(c) if c.is_alphanumeric() || c == '_' || c == '.' || c == '$')
        {
            text.push(self.advance().unwrap());
        }
        Ok(Token { kind: TokenKind::Ident(text), pos: start })
    }
}
