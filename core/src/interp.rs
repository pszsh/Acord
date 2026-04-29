use std::collections::HashMap;

// --- Values ---

#[derive(Clone, Debug)]
pub enum Value {
    Number(f64),
    Bool(bool),
    Str(String),
    Array(Vec<Value>),
    Void,
    Error(String),
}

impl Value {
    pub fn display(&self) -> String {
        match self {
            Value::Number(n) => format_number(*n),
            Value::Bool(b) => b.to_string(),
            Value::Str(s) => s.clone(),
            Value::Array(items) => {
                if items.len() == 2 {
                    if let (Value::Number(n), Value::Str(u)) = (&items[0], &items[1]) {
                        return format_spice(*n, u);
                    }
                }
                let inner: Vec<String> = items.iter().map(|v| match v {
                    Value::Str(s) => format!("\"{}\"", s),
                    other => other.display(),
                }).collect();
                format!("[{}]", inner.join(", "))
            }
            Value::Void => String::new(),
            Value::Error(e) => format!("error: {}", e),
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Value::Error(_))
    }

    fn truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::Void => false,
            Value::Error(_) => false,
        }
    }
}

fn format_number(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let s = format!("{:.10}", n);
        let s = s.trim_end_matches('0');
        let s = s.trim_end_matches('.');
        s.to_string()
    }
}

/// renders a scalar+unit value in SPICE notation.
fn format_spice(n: f64, unit: &str) -> String {
    if n == 0.0 {
        return format!("0{}", unit);
    }
    if !n.is_finite() {
        return format!("{}{}", n, unit);
    }
    let abs_n = n.abs();
    let (prefix, scale): (&str, f64) = if abs_n >= 1.0 {
        ("", 1.0)
    } else if abs_n >= 1e-3 {
        ("M", 1e-3)
    } else if abs_n >= 1e-6 {
        ("U", 1e-6)
    } else if abs_n >= 1e-9 {
        ("N", 1e-9)
    } else {
        ("P", 1e-12)
    };
    let mantissa = n / scale;
    let mag = mantissa.abs().log10().floor() as i32;
    let decimals = (2 - mag).max(0) as usize;
    let raw = format!("{:.*}", decimals, mantissa);
    let trimmed: &str = if raw.contains('.') {
        raw.trim_end_matches('0').trim_end_matches('.')
    } else {
        raw.as_str()
    };
    let compound = unit.chars().any(|c| !c.is_ascii_alphabetic());
    let sep = if compound && !unit.is_empty() { " " } else { "" };
    format!("{}{}{}{}", trimmed, prefix, sep, unit)
}

/// peels a spice-shaped value down to (scalar, unit).
fn unwrap_spice(v: &Value) -> (Value, Option<String>) {
    if let Value::Array(a) = v {
        if a.len() == 2 {
            if let (Value::Number(_), Value::Str(u)) = (&a[0], &a[1]) {
                return (a[0].clone(), Some(u.clone()));
            }
        }
    }
    (v.clone(), None)
}

/// re-wraps a numeric result with a carried unit, or drops the tag for non-numbers.
fn retag_spice(v: Value, unit: Option<String>) -> Value {
    match (&v, unit) {
        (Value::Number(_), Some(u)) => Value::Array(vec![v, Value::Str(u)]),
        _ => v,
    }
}

fn combine_unit_mul(a: &str, b: &str) -> Option<String> {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => None,
        (true, false) => Some(b.to_string()),
        (false, true) => Some(a.to_string()),
        (false, false) if a == b => Some(format!("{}²", a)),
        (false, false) => Some(format!("{}·{}", a, b)),
    }
}

fn combine_unit_div(a: &str, b: &str) -> Option<String> {
    if a == b { return None; }
    if b.is_empty() {
        return if a.is_empty() { None } else { Some(a.to_string()) };
    }
    if a.is_empty() { return Some(format!("1/{}", b)); }
    Some(format!("{}/{}", a, b))
}

fn combine_unit_pow(a: &str, exp: f64) -> Option<String> {
    if a.is_empty() { return None; }
    if exp == 1.0 { return Some(a.to_string()); }
    if exp == 2.0 { return Some(format!("{}²", a)); }
    if exp == 3.0 { return Some(format!("{}³", a)); }
    if exp == 0.5 { return Some(format!("√{}", a)); }
    if exp == exp.trunc() && exp.abs() < 1e9 {
        return Some(format!("{}^{}", a, exp as i64));
    }
    Some(format!("{}^{}", a, exp))
}

fn combine_unit_additive(a: &str, b: &str) -> Option<String> {
    if a == b {
        if a.is_empty() { None } else { Some(a.to_string()) }
    } else if a.is_empty() {
        Some(b.to_string())
    } else if b.is_empty() {
        Some(a.to_string())
    } else {
        None
    }
}

/// parses a cell address like `A1` into 0-based `(col, row)`.
pub fn parse_cell_address(s: &str) -> Option<(u32, u32)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut letters = String::new();
    let mut digits = String::new();
    for c in s.chars() {
        if c.is_ascii_alphabetic() && digits.is_empty() {
            letters.push(c);
        } else if c.is_ascii_digit() {
            digits.push(c);
        } else {
            return None;
        }
    }
    if letters.is_empty() || digits.is_empty() {
        return None;
    }
    let col = col_letters_to_index(&letters)?;
    let row_1based: u32 = digits.parse().ok()?;
    if row_1based == 0 {
        return None;
    }
    Some((col, row_1based - 1))
}

fn col_letters_to_index(s: &str) -> Option<u32> {
    let mut result: u32 = 0;
    for c in s.chars() {
        if !c.is_ascii_alphabetic() {
            return None;
        }
        let upper = c.to_ascii_uppercase();
        result = result.checked_mul(26)?.checked_add((upper as u32) - ('A' as u32) + 1)?;
    }
    if result == 0 {
        return None;
    }
    Some(result - 1)
}

/// renders a 0-based (col, row) as a spreadsheet-style address.
pub fn display_addr(col: u32, row: u32) -> String {
    let mut letters = String::new();
    let mut c = col as i64;
    loop {
        let rem = (c % 26) as u8;
        letters.insert(0, (b'A' + rem) as char);
        c = c / 26 - 1;
        if c < 0 {
            break;
        }
    }
    format!("{}{}", letters, row + 1)
}

/// coerces a raw cell string into a Value.
fn coerce_cell_value(s: &str) -> Value {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Value::Str(String::new());
    }
    if let Ok(n) = trimmed.parse::<f64>() {
        return Value::Number(n);
    }
    Value::Str(s.to_string())
}

fn rows_to_value(rows: &[Vec<String>]) -> Value {
    let outer: Vec<Value> = rows.iter().map(|row| {
        let inner: Vec<Value> = row.iter().map(|c| coerce_cell_value(c)).collect();
        Value::Array(inner)
    }).collect();
    Value::Array(outer)
}

// --- Tokens ---

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Spice(f64, String),
    Str(String),
    Bool(bool),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Eq,
    EqEq,
    BangEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
    Bang,
    Tilde,
    Colon,
    DotDot,
    Arrow,
    At,
    Let,
    While,
    Fn,
    If,
    Else,
    For,
    In,
    Return,
    Is,
    Use,
    ColonColon,
    Newline,
    Eof,
}

/// returns true if the source declares `use spice` or a `use spice::…` import.
fn source_enables_spice(src: &str) -> bool {
    src.lines().any(|l| {
        let t = l.trim();
        if t == "use spice" { return true; }
        if let Some(rest) = t.strip_prefix("use spice::") {
            !rest.is_empty() && rest.chars().all(|c| c.is_alphanumeric() || c == '_')
        } else {
            false
        }
    })
}

const SPICE_UNITS: &[&str] = &["F", "H", "HZ", "V", "A", "W", "R", "OHM", "S", "J"];

fn spice_prefix_scale(c: char) -> Option<f64> {
    match c {
        'm' | 'M' => Some(1e-3),
        'u' | 'U' | 'µ' | 'μ' => Some(1e-6),
        'n' | 'N' => Some(1e-9),
        'p' | 'P' => Some(1e-12),
        _ => None,
    }
}

/// parses a post-number alpha run as `(scale, unit_uppercase)`.
fn parse_spice_suffix(alpha: &str) -> Option<(f64, String)> {
    if alpha.is_empty() {
        return None;
    }
    let normalized: String = alpha.chars().map(|c| match c {
        'µ' | 'μ' => 'U',
        c => c.to_ascii_uppercase(),
    }).collect();

    if normalized.len() == 1 {
        let first = alpha.chars().next().unwrap();
        if let Some(scale) = spice_prefix_scale(first) {
            return Some((scale, String::new()));
        }
    }
    if SPICE_UNITS.iter().any(|u| *u == normalized) {
        return Some((1.0, normalized));
    }
    let first = alpha.chars().next().unwrap();
    if let Some(scale) = spice_prefix_scale(first) {
        let rest: String = normalized.chars().skip(1).collect();
        if SPICE_UNITS.iter().any(|u| *u == rest) {
            return Some((scale, rest));
        }
    }
    None
}

/// consumes an optional `e[+-]?DIGITS` and returns the multiplier.
fn try_consume_exponent(chars: &[char], i: &mut usize) -> f64 {
    let len = chars.len();
    if *i >= len { return 1.0; }
    if chars[*i] != 'e' && chars[*i] != 'E' { return 1.0; }
    let mut j = *i + 1;
    if j < len && (chars[j] == '+' || chars[j] == '-') { j += 1; }
    if j >= len || !chars[j].is_ascii_digit() { return 1.0; }
    while j < len && chars[j].is_ascii_digit() { j += 1; }
    let exp: i32 = chars[*i + 1..j].iter().collect::<String>()
        .parse().unwrap_or(0);
    *i = j;
    10f64.powi(exp)
}

/// pushes a Number or Spice token plus any implicit-multiplication `Star`.
fn finalize_number(
    tokens: &mut Vec<Token>,
    mut value: f64,
    chars: &[char],
    i: &mut usize,
    spice: bool,
) {
    value *= try_consume_exponent(chars, i);
    let len = chars.len();

    let run_start = *i;
    let mut run_end = run_start;
    while run_end < len && (chars[run_end].is_alphabetic() || chars[run_end] == 'µ' || chars[run_end] == 'μ') {
        run_end += 1;
    }

    if spice && run_end > run_start {
        let run: String = chars[run_start..run_end].iter().collect();
        if let Some((scale, unit)) = parse_spice_suffix(&run) {
            tokens.push(Token::Spice(value * scale, unit));
            *i = run_end;
            return;
        }
    }
    tokens.push(Token::Number(value));
    if *i < len {
        let c = chars[*i];
        if c.is_alphabetic() || c == '_' || c == '(' || c == 'µ' || c == 'μ' {
            tokens.push(Token::Star);
        }
    }
}

fn tokenize(input: &str, spice: bool) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\r' => { i += 1; }
            '\n' => { tokens.push(Token::Newline); i += 1; }
            '+' => { tokens.push(Token::Plus); i += 1; }
            '-' => {
                if i + 1 < len && chars[i + 1] == '>' {
                    tokens.push(Token::Arrow);
                    i += 2;
                    continue;
                }
                if i + 1 < len && (chars[i + 1].is_ascii_digit() || chars[i + 1] == '.') {
                    let can_be_neg = if tokens.is_empty() {
                        true
                    } else {
                        matches!(tokens.last(), Some(
                            Token::Plus | Token::Minus | Token::Star | Token::Slash |
                            Token::Percent | Token::Caret | Token::LParen | Token::LBracket |
                            Token::Comma | Token::Eq | Token::EqEq | Token::BangEq |
                            Token::Lt | Token::Gt | Token::LtEq | Token::GtEq |
                            Token::And | Token::Or | Token::Bang | Token::Tilde |
                            Token::Newline | Token::Colon
                        ))
                    };
                    if can_be_neg {
                        let start = i;
                        i += 1;
                        while i < len && (chars[i].is_ascii_digit() || (chars[i] == '.' && !(i + 1 < len && chars[i + 1] == '.'))) {
                            i += 1;
                        }
                        let s: String = chars[start..i].iter().collect();
                        let n: f64 = s.parse().map_err(|_| format!("invalid number: {}", s))?;
                        finalize_number(&mut tokens, n, &chars, &mut i, spice);
                    } else {
                        tokens.push(Token::Minus);
                        i += 1;
                    }
                } else {
                    tokens.push(Token::Minus);
                    i += 1;
                }
            }
            '*' => { tokens.push(Token::Star); i += 1; }
            '/' => {
                if i + 1 < len && chars[i + 1] == '/' {
                    while i < len && chars[i] != '\n' { i += 1; }
                } else {
                    tokens.push(Token::Slash);
                    i += 1;
                }
            }
            '%' => { tokens.push(Token::Percent); i += 1; }
            '^' => { tokens.push(Token::Caret); i += 1; }
            '(' => { tokens.push(Token::LParen); i += 1; }
            ')' => { tokens.push(Token::RParen); i += 1; }
            '{' => { tokens.push(Token::LBrace); i += 1; }
            '}' => { tokens.push(Token::RBrace); i += 1; }
            '[' => { tokens.push(Token::LBracket); i += 1; }
            ']' => { tokens.push(Token::RBracket); i += 1; }
            ',' => { tokens.push(Token::Comma); i += 1; }
            ':' => {
                if i + 1 < len && chars[i + 1] == ':' {
                    tokens.push(Token::ColonColon); i += 2;
                } else {
                    tokens.push(Token::Colon); i += 1;
                }
            }
            '.' if i + 1 < len && chars[i + 1] == '.' => {
                tokens.push(Token::DotDot); i += 2;
            }
            '!' => {
                if i + 1 < len && chars[i + 1] == '=' {
                    tokens.push(Token::BangEq); i += 2;
                } else {
                    tokens.push(Token::Bang); i += 1;
                }
            }
            '~' => { tokens.push(Token::Tilde); i += 1; }
            '@' => { tokens.push(Token::At); i += 1; }
            '=' => {
                if i + 1 < len && chars[i + 1] == '=' {
                    tokens.push(Token::EqEq); i += 2;
                } else {
                    tokens.push(Token::Eq); i += 1;
                }
            }
            '<' => {
                if i + 1 < len && chars[i + 1] == '=' {
                    tokens.push(Token::LtEq); i += 2;
                } else {
                    tokens.push(Token::Lt); i += 1;
                }
            }
            '>' => {
                if i + 1 < len && chars[i + 1] == '=' {
                    tokens.push(Token::GtEq); i += 2;
                } else {
                    tokens.push(Token::Gt); i += 1;
                }
            }
            '&' => {
                if i + 1 < len && chars[i + 1] == '&' {
                    tokens.push(Token::And); i += 2;
                } else {
                    return Err("unexpected '&', did you mean '&&'?".into());
                }
            }
            '|' => {
                if i + 1 < len && chars[i + 1] == '|' {
                    tokens.push(Token::Or); i += 2;
                } else {
                    return Err("unexpected '|', did you mean '||'?".into());
                }
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                while i < len && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < len {
                        i += 1;
                        match chars[i] {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            '\\' => s.push('\\'),
                            '"' => s.push('"'),
                            other => { s.push('\\'); s.push(other); }
                        }
                    } else {
                        s.push(chars[i]);
                    }
                    i += 1;
                }
                if i >= len {
                    return Err("unterminated string".into());
                }
                i += 1;
                tokens.push(Token::Str(s));
            }
            _ if c.is_ascii_digit() || (c == '.' && i + 1 < len && chars[i + 1].is_ascii_digit()) => {
                let start = i;
                while i < len && (chars[i].is_ascii_digit() || (chars[i] == '.' && !(i + 1 < len && chars[i + 1] == '.'))) {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                let n: f64 = s.parse().map_err(|_| format!("invalid number: {}", s))?;
                finalize_number(&mut tokens, n, &chars, &mut i, spice);
            }
            _ if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                match word.as_str() {
                    "let" => tokens.push(Token::Let),
                    "while" => tokens.push(Token::While),
                    "fn" => tokens.push(Token::Fn),
                    "if" => tokens.push(Token::If),
                    "else" => tokens.push(Token::Else),
                    "for" => tokens.push(Token::For),
                    "in" => tokens.push(Token::In),
                    "return" => tokens.push(Token::Return),
                    "true" => tokens.push(Token::Bool(true)),
                    "false" => tokens.push(Token::Bool(false)),
                    "and" => tokens.push(Token::And),
                    "or" => tokens.push(Token::Or),
                    "not" => tokens.push(Token::Bang),
                    "is" => tokens.push(Token::Is),
                    "use" => tokens.push(Token::Use),
                    _ => tokens.push(Token::Ident(word)),
                }
            }
            _ => {
                return Err(format!("unexpected character: '{}'", c));
            }
        }
    }
    tokens.push(Token::Eof);
    Ok(tokens)
}

// --- AST ---

#[derive(Debug, Clone)]
enum Op {
    Add, Sub, Mul, Div, Mod, Pow,
    Eq, Neq, Lt, Gt, Lte, Gte,
    And, Or, Not, Neg,
    Strip,
}

#[derive(Debug, Clone)]
enum Stmt {
    Let(String, Option<String>, Expr),
    Assign(String, Expr),
    While(Expr, Vec<Stmt>),
    IfElse(Expr, Vec<Stmt>, Option<Vec<Stmt>>),
    ForLoop(String, Expr, Vec<Stmt>),
    FnDef {
        name: String,
        params: Vec<(String, Option<String>)>,
        return_type: Option<String>,
        body: Vec<Stmt>,
    },
    Return(Expr),
    Use(String, Option<String>),
    CellAssign {
        block: Option<String>,
        table: String,
        cell: (u32, u32),
        value: Expr,
    },
    SolveDef {
        name: String,
        params: Vec<String>,
        target_var: String,
        source_fn: String,
        source_args: Vec<String>,
        result_var: String,
    },
    ExprStmt(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CellRefTarget {
    Whole,
    Cell(u32, u32),
    Range(u32, u32, u32, u32),
}

#[derive(Debug, Clone)]
enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    Ident(String),
    BinOp(Op, Box<Expr>, Box<Expr>),
    UnaryOp(Op, Box<Expr>),
    Call(String, Vec<Expr>),
    Array(Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Range(Box<Expr>, Box<Expr>),
    IsCheck(Box<Expr>, String),
    CellRef {
        block: Option<String>,
        table: Option<String>,
        target: CellRefTarget,
    },
    SolveMacro {
        var: String,
        source_fn: String,
    },
}

// --- Parser ---

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof);
        self.pos += 1;
        tok
    }

    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        let tok = self.advance();
        if &tok == expected {
            Ok(())
        } else {
            Err(format!("expected {:?}, got {:?}", expected, tok))
        }
    }

    fn skip_newlines(&mut self) {
        while self.peek() == &Token::Newline {
            self.advance();
        }
    }

    fn parse_program(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        self.skip_newlines();
        while self.peek() != &Token::Eof {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines();
        }
        Ok(stmts)
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect(&Token::LBrace)?;
        self.skip_newlines();
        let mut stmts = Vec::new();
        while self.peek() != &Token::RBrace && self.peek() != &Token::Eof {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines();
        }
        self.expect(&Token::RBrace)?;
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        self.skip_newlines();
        match self.peek().clone() {
            Token::Let => self.parse_let(),
            Token::While => self.parse_while(),
            Token::If => self.parse_if(),
            Token::For => self.parse_for(),
            Token::Return => self.parse_return(),
            Token::Fn => self.parse_fn_def(),
            Token::Use => self.parse_use(),
            Token::At => {
                let saved = self.pos;
                let cref = self.parse_cell_ref()?;
                if self.peek() == &Token::Eq {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.skip_newlines();
                    return match cref {
                        Expr::CellRef { block, table: Some(table), target: CellRefTarget::Cell(col, row) } => {
                            Ok(Stmt::CellAssign { block, table, cell: (col, row), value })
                        }
                        Expr::CellRef { target: CellRefTarget::Whole, .. } => {
                            Err("cannot assign to a whole table — use @Table:A1 = ... to write a single cell".into())
                        }
                        Expr::CellRef { target: CellRefTarget::Range(..), .. } => {
                            Err("cannot assign to a range — use @Table:A1 = ... for a single cell".into())
                        }
                        _ => Err("cell assignment requires @Table:A1 = ... form".into()),
                    };
                }
                self.pos = saved;
                let expr = self.parse_expr()?;
                self.skip_newlines();
                Ok(Stmt::ExprStmt(expr))
            }
            Token::Ident(_) => {
                let saved = self.pos;
                if let Token::Ident(name) = self.advance() {
                    if self.peek() == &Token::LParen {
                        let paren_saved = self.pos;
                        self.advance();
                        let mut params = Vec::new();
                        let mut valid = true;
                        if self.peek() != &Token::RParen {
                            match self.peek() {
                                Token::Ident(_) => {
                                    if let Token::Ident(p) = self.advance() { params.push(p); }
                                    while self.peek() == &Token::Comma {
                                        self.advance();
                                        if let Token::Ident(p) = self.advance() {
                                            params.push(p);
                                        } else {
                                            valid = false;
                                            break;
                                        }
                                    }
                                }
                                _ => { valid = false; }
                            }
                        }
                        if valid && self.peek() == &Token::RParen {
                            self.advance();
                            if self.peek() == &Token::Eq {
                                self.advance();
                                let body_expr = self.parse_expr()?;
                                self.skip_newlines();
                                let typed_params: Vec<(String, Option<String>)> =
                                    params.into_iter().map(|p| (p, None)).collect();
                                return Ok(Stmt::FnDef {
                                    name,
                                    params: typed_params,
                                    return_type: None,
                                    body: vec![Stmt::ExprStmt(body_expr)],
                                });
                            }
                        }
                        self.pos = paren_saved;
                    }
                    if self.peek() == &Token::Eq {
                        self.advance();
                        let expr = self.parse_expr()?;
                        self.skip_newlines();
                        return Ok(Stmt::Assign(name, expr));
                    }
                }
                self.pos = saved;
                let expr = self.parse_expr()?;
                self.skip_newlines();
                Ok(Stmt::ExprStmt(expr))
            }
            _ => {
                let expr = self.parse_expr()?;
                self.skip_newlines();
                Ok(Stmt::ExprStmt(expr))
            }
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::Let)?;
        let name = match self.advance() {
            Token::Ident(n) => n,
            t => return Err(format!("expected identifier after 'let', got {:?}", t)),
        };
        if self.peek() == &Token::LParen {
            return self.parse_let_with_params(name);
        }
        let type_ann = if self.peek() == &Token::Colon {
            self.advance();
            match self.advance() {
                Token::Ident(t) => Some(t),
                t => return Err(format!("expected type name after ':', got {:?}", t)),
            }
        } else {
            None
        };
        self.expect(&Token::Eq)?;
        let expr = self.parse_expr()?;
        self.skip_newlines();
        Ok(Stmt::Let(name, type_ann, expr))
    }

    fn parse_let_with_params(&mut self, name: String) -> Result<Stmt, String> {
        self.expect(&Token::LParen)?;
        let mut params = Vec::new();
        if self.peek() != &Token::RParen {
            match self.advance() {
                Token::Ident(p) => params.push(p),
                t => return Err(format!("expected parameter name, got {:?}", t)),
            }
            while self.peek() == &Token::Comma {
                self.advance();
                match self.advance() {
                    Token::Ident(p) => params.push(p),
                    t => return Err(format!("expected parameter name, got {:?}", t)),
                }
            }
        }
        self.expect(&Token::RParen)?;
        self.expect(&Token::Eq)?;
        let rhs = self.parse_expr()?;

        let is_where = matches!(self.peek(), Token::Ident(w) if w == "where");
        if is_where {
            let target_var = match rhs {
                Expr::Ident(s) => s,
                _ => return Err("expected a single target variable before 'where'".into()),
            };
            self.advance();
            let source_fn = match self.advance() {
                Token::Ident(n) => n,
                t => return Err(format!("expected source function name after 'where', got {:?}", t)),
            };
            self.expect(&Token::LParen)?;
            let mut source_args = Vec::new();
            if self.peek() != &Token::RParen {
                match self.advance() {
                    Token::Ident(a) => source_args.push(a),
                    t => return Err(format!("expected argument name, got {:?}", t)),
                }
                while self.peek() == &Token::Comma {
                    self.advance();
                    match self.advance() {
                        Token::Ident(a) => source_args.push(a),
                        t => return Err(format!("expected argument name, got {:?}", t)),
                    }
                }
            }
            self.expect(&Token::RParen)?;
            self.expect(&Token::Eq)?;
            let result_var = match self.advance() {
                Token::Ident(n) => n,
                t => return Err(format!("expected result variable after '=', got {:?}", t)),
            };
            self.skip_newlines();
            return Ok(Stmt::SolveDef {
                name,
                params,
                target_var,
                source_fn,
                source_args,
                result_var,
            });
        }

        self.skip_newlines();
        let typed_params: Vec<(String, Option<String>)> =
            params.into_iter().map(|p| (p, None)).collect();
        Ok(Stmt::FnDef {
            name,
            params: typed_params,
            return_type: None,
            body: vec![Stmt::ExprStmt(rhs)],
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::While)?;
        let has_paren = if self.peek() == &Token::LParen {
            self.advance();
            true
        } else {
            false
        };
        let cond = self.parse_expr()?;
        if has_paren {
            self.expect(&Token::RParen)?;
        }
        self.skip_newlines();
        let body = self.parse_block()?;
        Ok(Stmt::While(cond, body))
    }

    fn parse_if(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::If)?;
        let has_paren = if self.peek() == &Token::LParen {
            self.advance();
            true
        } else {
            false
        };
        let cond = self.parse_expr()?;
        if has_paren {
            self.expect(&Token::RParen)?;
        }
        self.skip_newlines();
        let then_body = self.parse_block()?;
        self.skip_newlines();
        let else_body = if self.peek() == &Token::Else {
            self.advance();
            self.skip_newlines();
            if self.peek() == &Token::If {
                Some(vec![self.parse_if()?])
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };
        Ok(Stmt::IfElse(cond, then_body, else_body))
    }

    fn parse_for(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::For)?;
        let var = match self.advance() {
            Token::Ident(n) => n,
            t => return Err(format!("expected loop variable, got {:?}", t)),
        };
        self.expect(&Token::In)?;
        let iter = self.parse_expr()?;
        self.skip_newlines();
        let body = self.parse_block()?;
        Ok(Stmt::ForLoop(var, iter, body))
    }

    fn parse_return(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::Return)?;
        if matches!(self.peek(), Token::Newline | Token::Eof | Token::RBrace) {
            return Ok(Stmt::Return(Expr::Bool(false)));
        }
        let expr = self.parse_expr()?;
        self.skip_newlines();
        Ok(Stmt::Return(expr))
    }

    fn parse_use(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::Use)?;
        let module = match self.advance() {
            Token::Ident(name) => name,
            other => return Err(format!("expected module name after 'use', got {:?}", other)),
        };
        let item = if self.peek() == &Token::ColonColon {
            self.advance();
            match self.advance() {
                Token::Ident(name) => Some(name),
                Token::Star => Some("*".to_string()),
                other => return Err(format!("expected item name after '::', got {:?}", other)),
            }
        } else {
            None
        };
        self.skip_newlines();
        Ok(Stmt::Use(module, item))
    }

    fn parse_cell_ref(&mut self) -> Result<Expr, String> {
        self.expect(&Token::At)?;
        let first = match self.advance() {
            Token::Ident(name) => name.to_lowercase(),
            t => return Err(format!("expected identifier after '@', got {:?}", t)),
        };

        let (block, table) = if self.peek() == &Token::ColonColon {
            self.advance();
            let tname = match self.advance() {
                Token::Ident(name) => name.to_lowercase(),
                t => return Err(format!("expected table name after '::', got {:?}", t)),
            };
            (Some(first), Some(tname))
        } else {
            (None, Some(first))
        };

        let target = if self.peek() == &Token::Colon {
            self.advance();
            let (c1, r1) = self.parse_cell_addr_token()?;
            if self.peek() == &Token::Colon {
                self.advance();
                let (c2, r2) = self.parse_cell_addr_token()?;
                CellRefTarget::Range(c1, r1, c2, r2)
            } else {
                CellRefTarget::Cell(c1, r1)
            }
        } else if self.peek() == &Token::LBracket {
            self.advance();
            let (c1, r1) = self.parse_cell_addr_token()?;
            self.expect(&Token::Colon)?;
            let (c2, r2) = self.parse_cell_addr_token()?;
            self.expect(&Token::RBracket)?;
            CellRefTarget::Range(c1, r1, c2, r2)
        } else {
            CellRefTarget::Whole
        };

        Ok(Expr::CellRef { block, table, target })
    }

    fn parse_solve_macro(&mut self) -> Result<Expr, String> {
        match self.advance() {
            Token::Ident(n) if n == "solve" => {}
            t => return Err(format!("expected 'solve', got {:?}", t)),
        }
        self.expect(&Token::Bang)?;
        self.expect(&Token::LParen)?;
        let var = match self.advance() {
            Token::Ident(n) => n,
            t => return Err(format!("expected target variable after 'solve!(', got {:?}", t)),
        };
        match self.peek().clone() {
            Token::Comma => { self.advance(); }
            Token::Ident(ref n) if n == "from" => { self.advance(); }
            t => return Err(format!("expected ',' or 'from' in solve!(...), got {:?}", t)),
        }
        let source_fn = match self.advance() {
            Token::Ident(n) => n,
            t => return Err(format!("expected source function name in solve!(...), got {:?}", t)),
        };
        self.expect(&Token::RParen)?;
        Ok(Expr::SolveMacro { var, source_fn })
    }

    fn parse_cell_addr_token(&mut self) -> Result<(u32, u32), String> {
        let name = match self.advance() {
            Token::Ident(n) => n,
            t => return Err(format!("expected cell address, got {:?}", t)),
        };
        parse_cell_address(&name).ok_or_else(|| format!("invalid cell address: {}", name))
    }

    fn parse_fn_def(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::Fn)?;
        let name = match self.advance() {
            Token::Ident(n) => n,
            t => return Err(format!("expected function name, got {:?}", t)),
        };
        self.expect(&Token::LParen)?;
        let mut params: Vec<(String, Option<String>)> = Vec::new();
        if self.peek() != &Token::RParen {
            params.push(self.parse_typed_param()?);
            while self.peek() == &Token::Comma {
                self.advance();
                params.push(self.parse_typed_param()?);
            }
        }
        self.expect(&Token::RParen)?;
        let return_type = if self.peek() == &Token::Arrow {
            self.advance();
            match self.advance() {
                Token::Ident(t) => Some(t),
                t => return Err(format!("expected return type after '->', got {:?}", t)),
            }
        } else {
            None
        };
        self.skip_newlines();
        let body = self.parse_block()?;
        Ok(Stmt::FnDef { name, params, return_type, body })
    }

    fn parse_typed_param(&mut self) -> Result<(String, Option<String>), String> {
        let name = match self.advance() {
            Token::Ident(p) => p,
            t => return Err(format!("expected parameter name, got {:?}", t)),
        };
        let ty = if self.peek() == &Token::Colon {
            self.advance();
            match self.advance() {
                Token::Ident(t) => Some(t),
                t => return Err(format!("expected type after ':', got {:?}", t)),
            }
        } else {
            None
        };
        Ok((name, ty))
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        let left = self.parse_or()?;
        if self.peek() == &Token::DotDot {
            self.advance();
            let right = self.parse_or()?;
            return Ok(Expr::Range(Box::new(left), Box::new(right)));
        }
        if self.peek() == &Token::Colon {
            if let Expr::Ident(ref start_name) = left {
                if let Some((c0, r0)) = parse_cell_address(start_name) {
                    let saved = self.pos;
                    self.advance();
                    if let Token::Ident(end_name) = self.peek().clone() {
                        if let Some((c1, r1)) = parse_cell_address(&end_name) {
                            self.advance();
                            return Ok(Expr::CellRef {
                                block: None,
                                table: None,
                                target: CellRefTarget::Range(c0, r0, c1, r1),
                            });
                        }
                    }
                    self.pos = saved;
                }
            }
        }
        Ok(left)
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while self.peek() == &Token::Or {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinOp(Op::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_comparison()?;
        while self.peek() == &Token::And {
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::BinOp(Op::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_additive()?;
        loop {
            if self.peek() == &Token::Is {
                self.advance();
                let type_name = match self.advance() {
                    Token::Ident(t) => t,
                    t => {
                        return Err(format!(
                            "expected type name after 'is', got {:?}",
                            t
                        ));
                    }
                };
                left = Expr::IsCheck(Box::new(left), type_name);
                continue;
            }
            let op = match self.peek() {
                Token::EqEq => Op::Eq,
                Token::BangEq => Op::Neq,
                Token::Lt => Op::Lt,
                Token::Gt => Op::Gt,
                Token::LtEq => Op::Lte,
                Token::GtEq => Op::Gte,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            left = Expr::BinOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Token::Plus => Op::Add,
                Token::Minus => Op::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = Expr::BinOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_power()?;
        loop {
            let op = match self.peek() {
                Token::Star => Op::Mul,
                Token::Slash => Op::Div,
                Token::Percent => Op::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_power()?;
            left = Expr::BinOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> Result<Expr, String> {
        let base = self.parse_unary()?;
        if self.peek() == &Token::Caret {
            self.advance();
            let exp = self.parse_power()?;
            Ok(Expr::BinOp(Op::Pow, Box::new(base), Box::new(exp)))
        } else {
            Ok(base)
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Token::Bang => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp(Op::Not, Box::new(expr)))
            }
            Token::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp(Op::Neg, Box::new(expr)))
            }
            Token::Tilde => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp(Op::Strip, Box::new(expr)))
            }
            _ => self.parse_call(),
        }
    }

    fn parse_call(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_atom()?;
        if let Expr::Ident(ref name) = expr {
            if self.peek() == &Token::LParen {
                self.advance();
                let mut args = Vec::new();
                if self.peek() != &Token::RParen {
                    args.push(self.parse_expr()?);
                    while self.peek() == &Token::Comma {
                        self.advance();
                        args.push(self.parse_expr()?);
                    }
                }
                self.expect(&Token::RParen)?;
                expr = Expr::Call(name.clone(), args);
            }
        }
        while self.peek() == &Token::LBracket {
            self.advance();
            let index = self.parse_expr()?;
            self.expect(&Token::RBracket)?;
            expr = Expr::Index(Box::new(expr), Box::new(index));
        }
        Ok(expr)
    }

    fn parse_atom(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Token::Number(n) => { self.advance(); Ok(Expr::Num(n)) }
            Token::Spice(n, unit) => {
                self.advance();
                Ok(Expr::Array(vec![Expr::Num(n), Expr::Str(unit)]))
            }
            Token::Str(s) => { self.advance(); Ok(Expr::Str(s)) }
            Token::Bool(b) => { self.advance(); Ok(Expr::Bool(b)) }
            Token::At => self.parse_cell_ref(),
            Token::Ident(name) => {
                if name == "solve"
                    && self.tokens.get(self.pos + 1) == Some(&Token::Bang)
                    && self.tokens.get(self.pos + 2) == Some(&Token::LParen)
                {
                    return self.parse_solve_macro();
                }
                self.advance();
                Ok(Expr::Ident(name))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Token::LBracket => {
                self.advance();
                let mut items = Vec::new();
                if self.peek() != &Token::RBracket {
                    items.push(self.parse_expr()?);
                    while self.peek() == &Token::Comma {
                        self.advance();
                        items.push(self.parse_expr()?);
                    }
                }
                self.expect(&Token::RBracket)?;
                Ok(Expr::Array(items))
            }
            t => Err(format!("unexpected token: {:?}", t)),
        }
    }
}

// --- Interpreter ---

#[derive(Clone, Debug)]
pub struct FnDef {
    params: Vec<(String, Option<String>)>,
    return_type: Option<String>,
    body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub struct SolvedFnDef {
    source_fn: String,
    solve_param_idx: usize,
    new_params: Vec<String>,
}

pub struct Interpreter {
    vars: HashMap<String, Value>,
    var_types: HashMap<String, String>,
    fns: HashMap<String, FnDef>,
    solved_fns: HashMap<String, SolvedFnDef>,
    spice_enabled: bool,
    tables: HashMap<String, Vec<Vec<String>>>,
    current_table: Option<String>,
    current_block: Option<String>,
    table_writes: Vec<TableWrite>,
}

#[derive(Debug, Clone)]
pub struct TableWrite {
    pub table_key: String,
    pub cell: (u32, u32),
    pub value: String,
}

const MAX_ITERATIONS: usize = 10_000;
const MAX_CALL_DEPTH: u32 = 256;

impl Interpreter {
    pub fn new() -> Self {
        Interpreter {
            vars: HashMap::new(),
            var_types: HashMap::new(),
            fns: HashMap::new(),
            solved_fns: HashMap::new(),
            spice_enabled: false,
            tables: HashMap::new(),
            current_table: None,
            current_block: None,
            table_writes: Vec::new(),
        }
    }

    /// registers a table's contents under `name` (lowercased).
    pub fn register_table(&mut self, name: &str, rows: Vec<Vec<String>>) {
        self.tables.insert(name.to_lowercase(), rows);
    }

    /// sets the table anchor for bare cell refs.
    pub fn set_current_table(&mut self, name: Option<&str>) {
        self.current_table = name.map(|s| s.to_lowercase());
    }

    /// sets the block anchor for unqualified block-scoped table names.
    pub fn set_current_block(&mut self, name: Option<&str>) {
        self.current_block = name.map(|s| s.to_lowercase());
    }

    /// drains cell writes accumulated during the last eval.
    pub fn drain_table_writes(&mut self) -> Vec<TableWrite> {
        std::mem::take(&mut self.table_writes)
    }

    /// overwrites a cell's raw string in the registered table without logging a write.
    pub fn write_cell_raw(&mut self, name: &str, col: u32, row: u32, value: &str) {
        let key = name.to_lowercase();
        if let Some(rows) = self.tables.get_mut(&key) {
            let r = row as usize;
            let c = col as usize;
            while rows.len() <= r { rows.push(Vec::new()); }
            while rows[r].len() <= c { rows[r].push(String::new()); }
            rows[r][c] = value.to_string();
        }
    }

    /// resolves a (block, table) pair to a registered HashMap key.
    fn resolve_table_key(&self, block: Option<&str>, table: Option<&str>) -> Option<String> {
        match (block, table) {
            (Some(b), Some(t)) => {
                let key = format!("{}::{}", b.to_lowercase(), t.to_lowercase());
                if self.tables.contains_key(&key) { Some(key) } else { None }
            }
            (None, Some(t)) => {
                let bare = t.to_lowercase();
                if self.tables.contains_key(&bare) { return Some(bare); }
                if let Some(ref b) = self.current_block {
                    let qualified = format!("{}::{}", b, bare);
                    if self.tables.contains_key(&qualified) { return Some(qualified); }
                }
                None
            }
            (None, None) => {
                self.current_table.clone().filter(|k| self.tables.contains_key(k))
            }
            (Some(_), None) => None,
        }
    }

    /// resolves a (block, table) pair, returning a synthesized key when unregistered.
    fn resolve_table_key_lenient(&self, block: Option<&str>, table: Option<&str>) -> Option<String> {
        match (block, table) {
            (Some(b), Some(t)) => Some(format!("{}::{}", b.to_lowercase(), t.to_lowercase())),
            (None, Some(t)) => {
                self.resolve_table_key(block, Some(t))
                    .or_else(|| Some(t.to_lowercase()))
            }
            (None, None) => self.current_table.clone(),
            (Some(_), None) => None,
        }
    }

    fn read_cell(&self, block: Option<&str>, table: Option<&str>, col: u32, row: u32) -> Result<Value, String> {
        let key = self.resolve_table_key(block, table)
            .ok_or_else(|| "cell ref with no table".to_string())?;
        let rows = self.tables.get(&key)
            .ok_or_else(|| format!("unknown table '{}'", key))?;
        let cell = rows.get(row as usize)
            .and_then(|r| r.get(col as usize))
            .ok_or_else(|| format!("cell {} out of bounds in '{}'", display_addr(col, row), key))?;
        Ok(coerce_cell_value(cell))
    }

    fn read_whole(&self, block: Option<&str>, table: Option<&str>) -> Result<Value, String> {
        let key = self.resolve_table_key(block, table)
            .ok_or_else(|| "table ref with no name".to_string())?;
        let rows = self.tables.get(&key)
            .ok_or_else(|| format!("unknown table '{}'", key))?;
        Ok(rows_to_value(rows))
    }

    fn read_range(&self, block: Option<&str>, table: Option<&str>,
                  c0: u32, r0: u32, c1: u32, r1: u32) -> Result<Value, String> {
        let key = self.resolve_table_key(block, table)
            .ok_or_else(|| "range ref with no table".to_string())?;
        let rows = self.tables.get(&key)
            .ok_or_else(|| format!("unknown table '{}'", key))?;
        let (cmin, cmax) = if c0 <= c1 { (c0, c1) } else { (c1, c0) };
        let (rmin, rmax) = if r0 <= r1 { (r0, r1) } else { (r1, r0) };
        let mut out_rows = Vec::with_capacity((rmax - rmin + 1) as usize);
        for r in rmin..=rmax {
            let src_row = rows.get(r as usize);
            let mut out_row = Vec::with_capacity((cmax - cmin + 1) as usize);
            for c in cmin..=cmax {
                let cell = src_row.and_then(|row| row.get(c as usize))
                    .map(|s| s.as_str())
                    .unwrap_or("");
                out_row.push(coerce_cell_value(cell));
            }
            out_rows.push(Value::Array(out_row));
        }
        Ok(Value::Array(out_rows))
    }

    pub fn exec_line(&mut self, line: &str) -> Result<Option<Value>, String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        if !self.spice_enabled && source_enables_spice(trimmed) {
            self.spice_enabled = true;
        }
        let tokens = tokenize(trimmed, self.spice_enabled)?;
        let mut parser = Parser::new(tokens);
        let stmts = parser.parse_program()?;
        let mut last = Value::Void;
        for stmt in stmts {
            last = self.exec_stmt(&stmt, 0)?;
        }
        match last {
            Value::Void => Ok(None),
            v => Ok(Some(v)),
        }
    }

    /// evaluates a pre-parsed cell formula.
    pub fn eval_formula(&mut self, f: &ParsedFormula) -> Result<Value, String> {
        self.eval_expr(&f.ast, 0)
    }

    pub fn eval_expr_str(&mut self, expr: &str) -> Result<Value, String> {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            return Err("empty expression".into());
        }
        let tokens = tokenize(trimmed, self.spice_enabled)?;
        let mut parser = Parser::new(tokens);
        let e = parser.parse_expr()?;
        self.eval_expr(&e, 0)
    }

    pub fn set_spice_enabled(&mut self, on: bool) {
        self.spice_enabled = on;
    }

    pub fn spice_enabled(&self) -> bool {
        self.spice_enabled
    }

    fn exec_stmt(&mut self, stmt: &Stmt, depth: u32) -> Result<Value, String> {
        match stmt {
            Stmt::Let(name, _type_ann, Expr::SolveMacro { var, source_fn }) => {
                let def = self.build_solved_fn_def(source_fn, var, None)?;
                self.fns.remove(name);
                self.var_types.remove(name);
                self.vars.remove(name);
                self.solved_fns.insert(name.clone(), def);
                Ok(Value::Void)
            }
            Stmt::SolveDef { name, params, target_var, source_fn, source_args, result_var } => {
                if params.first().map(|s| s.as_str()) != Some(result_var.as_str()) {
                    return Err(format!(
                        "inversion: result variable '{}' must be the first parameter of '{}'",
                        result_var, name
                    ));
                }
                let def = self.build_solved_fn_def(
                    source_fn,
                    target_var,
                    Some((params.as_slice(), source_args.as_slice())),
                )?;
                self.fns.remove(name);
                self.var_types.remove(name);
                self.vars.remove(name);
                self.solved_fns.insert(name.clone(), def);
                Ok(Value::Void)
            }
            Stmt::Let(name, type_ann, expr) => {
                let val = self.eval_expr(expr, depth)?;
                let val = match apply_type_annotation(&val, type_ann.as_deref()) {
                    Ok(v) => v,
                    Err(_) => {
                        let t = type_ann.as_deref().unwrap_or("?");
                        return Err(format!(
                            "cannot bind {} to '{}' as {}: not a clean conversion",
                            val.display(),
                            name,
                            t
                        ));
                    }
                };
                if let Some(t) = type_ann {
                    self.var_types.insert(name.clone(), t.clone());
                } else {
                    self.var_types.remove(name);
                }
                self.vars.insert(name.clone(), val);
                Ok(Value::Void)
            }
            Stmt::Assign(name, expr) => {
                let val = self.eval_expr(expr, depth)?;
                if let Some(t) = self.var_types.get(name).cloned() {
                    match apply_type_annotation(&val, Some(&t)) {
                        Ok(v) => {
                            self.vars.insert(name.clone(), v);
                        }
                        Err(_) => {
                            return Err(format!(
                                "cannot assign {} to '{}' (declared {}); value left unchanged",
                                val.display(),
                                name,
                                t
                            ));
                        }
                    }
                } else {
                    self.vars.insert(name.clone(), val);
                }
                Ok(Value::Void)
            }
            Stmt::While(cond, body) => {
                let mut iterations = 0;
                loop {
                    let cv = self.eval_expr(cond, depth)?;
                    if !cv.truthy() { break; }
                    iterations += 1;
                    if iterations > MAX_ITERATIONS {
                        return Err(format!("loop exceeded {} iterations", MAX_ITERATIONS));
                    }
                    let mut last = Value::Void;
                    for s in body {
                        last = self.exec_stmt(s, depth)?;
                    }
                    drop(last);
                }
                Ok(Value::Void)
            }
            Stmt::IfElse(cond, then_body, else_body) => {
                let cv = self.eval_expr(cond, depth)?;
                let body = if cv.truthy() { then_body } else {
                    match else_body { Some(b) => b, None => return Ok(Value::Void) }
                };
                let mut last = Value::Void;
                for s in body {
                    last = self.exec_stmt(s, depth)?;
                }
                Ok(last)
            }
            Stmt::ForLoop(var, iter_expr, body) => {
                let iterable = self.eval_expr(iter_expr, depth)?;
                let items = match iterable {
                    Value::Array(a) => a,
                    _ => return Err("for loop requires an array or range".into()),
                };
                let mut iterations = 0;
                let mut last = Value::Void;
                for item in &items {
                    iterations += 1;
                    if iterations > MAX_ITERATIONS {
                        return Err(format!("loop exceeded {} iterations", MAX_ITERATIONS));
                    }
                    self.vars.insert(var.clone(), item.clone());
                    for s in body {
                        last = self.exec_stmt(s, depth)?;
                    }
                }
                Ok(last)
            }
            Stmt::FnDef { name, params, return_type, body } => {
                self.solved_fns.remove(name);
                self.fns.insert(name.clone(), FnDef {
                    params: params.clone(),
                    return_type: return_type.clone(),
                    body: body.clone(),
                });
                Ok(Value::Void)
            }
            Stmt::Return(expr) => {
                let val = self.eval_expr(expr, depth)?;
                Err(format!("\x00return:{}", encode_return(&val)))
            }
            Stmt::Use(_, _) => {
                Ok(Value::Void)
            }
            Stmt::CellAssign { block, table, cell, value } => {
                let v = self.eval_expr(value, depth)?;
                let text = v.display();
                let key = self.resolve_table_key_lenient(block.as_deref(), Some(table))
                    .ok_or_else(|| format!("cannot assign: no table '{}'", table))?;
                if let Some(rows) = self.tables.get_mut(&key) {
                    let r = cell.1 as usize;
                    let c = cell.0 as usize;
                    while rows.len() <= r { rows.push(Vec::new()); }
                    while rows[r].len() <= c { rows[r].push(String::new()); }
                    rows[r][c] = text.clone();
                }
                self.table_writes.push(TableWrite {
                    table_key: key,
                    cell: *cell,
                    value: text,
                });
                Ok(Value::Void)
            }
            Stmt::ExprStmt(expr) => {
                self.eval_expr(expr, depth)
            }
        }
    }

    fn eval_expr(&mut self, expr: &Expr, depth: u32) -> Result<Value, String> {
        match expr {
            Expr::Num(n) => Ok(Value::Number(*n)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Ident(name) => {
                if let Some(v) = self.vars.get(name) {
                    return Ok(v.clone());
                }
                if let Some(v) = builtin_constant(name) {
                    return Ok(v);
                }
                if self.current_table.is_some() {
                    if let Some((col, row)) = parse_cell_address(name) {
                        return self.read_cell(None, None, col, row);
                    }
                }
                Err(format!("undefined variable '{}'", name))
            }
            Expr::SolveMacro { .. } => {
                Err("solve!(…) can only appear on the right-hand side of a 'let' binding".into())
            }
            Expr::CellRef { block, table, target } => {
                match target {
                    CellRefTarget::Cell(col, row) => {
                        self.read_cell(block.as_deref(), table.as_deref(), *col, *row)
                    }
                    CellRefTarget::Whole => {
                        self.read_whole(block.as_deref(), table.as_deref())
                    }
                    CellRefTarget::Range(c0, r0, c1, r1) => {
                        self.read_range(block.as_deref(), table.as_deref(), *c0, *r0, *c1, *r1)
                    }
                }
            }
            Expr::Array(items) => {
                let mut vals = Vec::new();
                for item in items {
                    vals.push(self.eval_expr(item, depth)?);
                }
                Ok(Value::Array(vals))
            }
            Expr::UnaryOp(Op::Not, inner) => {
                let v = self.eval_expr(inner, depth)?;
                Ok(Value::Bool(!v.truthy()))
            }
            Expr::UnaryOp(Op::Neg, inner) => {
                let v = self.eval_expr(inner, depth)?;
                match v {
                    Value::Number(n) => Ok(Value::Number(-n)),
                    _ => Err("cannot negate non-number".into()),
                }
            }
            Expr::UnaryOp(Op::Strip, inner) => {
                let v = self.eval_expr(inner, depth)?;
                Ok(match v {
                    Value::Bool(b) => Value::Number(if b { 1.0 } else { 0.0 }),
                    Value::Array(ref a) if a.len() == 2 => {
                        if let (Value::Number(n), Value::Str(_)) = (&a[0], &a[1]) {
                            Value::Number(*n)
                        } else {
                            v
                        }
                    }
                    other => other,
                })
            }
            Expr::UnaryOp(op, _) => Err(format!("invalid unary op: {:?}", op)),
            Expr::BinOp(op, lhs, rhs) => self.eval_binop(op, lhs, rhs, depth),
            Expr::Call(name, args) => self.eval_call(name, args, depth),
            Expr::Index(target, index) => {
                let target_val = self.eval_expr(target, depth)?;
                let index_val = self.eval_expr(index, depth)?;
                match (&target_val, &index_val) {
                    (Value::Array(arr), Value::Number(n)) => {
                        let i = *n as i64;
                        let idx = if i < 0 { (arr.len() as i64 + i) as usize } else { i as usize };
                        arr.get(idx).cloned().ok_or_else(|| format!("index {} out of bounds (len {})", i, arr.len()))
                    }
                    (Value::Str(s), Value::Number(n)) => {
                        let i = *n as i64;
                        let chars: Vec<char> = s.chars().collect();
                        let idx = if i < 0 { (chars.len() as i64 + i) as usize } else { i as usize };
                        chars.get(idx).map(|c| Value::Str(c.to_string()))
                            .ok_or_else(|| format!("index {} out of bounds (len {})", i, chars.len()))
                    }
                    _ => Err(format!("cannot index {} with {}", type_name(&target_val), type_name(&index_val))),
                }
            }
            Expr::IsCheck(inner, target) => {
                let v = self.eval_expr(inner, depth)?;
                Ok(Value::Bool(value_is_kind(&v, target)))
            }
            Expr::Range(start, end) => {
                let sv = self.eval_expr(start, depth)?;
                let ev = self.eval_expr(end, depth)?;
                match (&sv, &ev) {
                    (Value::Number(a), Value::Number(b)) => {
                        let a = *a as i64;
                        let b = *b as i64;
                        let items: Vec<Value> = (a..b).map(|n| Value::Number(n as f64)).collect();
                        if items.len() > MAX_ITERATIONS {
                            return Err(format!("range too large ({} elements)", items.len()));
                        }
                        Ok(Value::Array(items))
                    }
                    _ => Err("range requires two numbers".into()),
                }
            }
        }
    }

    fn eval_binop(&mut self, op: &Op, lhs: &Expr, rhs: &Expr, depth: u32) -> Result<Value, String> {
        if matches!(op, Op::And) {
            let l = self.eval_expr(lhs, depth)?;
            if !l.truthy() { return Ok(Value::Bool(false)); }
            let r = self.eval_expr(rhs, depth)?;
            return Ok(Value::Bool(r.truthy()));
        }
        if matches!(op, Op::Or) {
            let l = self.eval_expr(lhs, depth)?;
            if l.truthy() { return Ok(Value::Bool(true)); }
            let r = self.eval_expr(rhs, depth)?;
            return Ok(Value::Bool(r.truthy()));
        }

        let l_raw = self.eval_expr(lhs, depth)?;
        let r_raw = self.eval_expr(rhs, depth)?;
        let (l, l_unit) = unwrap_spice(&l_raw);
        let (r, r_unit) = unwrap_spice(&r_raw);
        let had_unit = l_unit.is_some() || r_unit.is_some();
        let la = l_unit.unwrap_or_default();
        let ra = r_unit.unwrap_or_default();
        let unit_after: Option<String> = if !had_unit {
            None
        } else {
            match op {
                Op::Add | Op::Sub | Op::Mod => combine_unit_additive(&la, &ra),
                Op::Mul => combine_unit_mul(&la, &ra),
                Op::Div => combine_unit_div(&la, &ra),
                Op::Pow => {
                    if let Value::Number(e) = r {
                        combine_unit_pow(&la, e)
                    } else if la.is_empty() {
                        None
                    } else {
                        Some(la.clone())
                    }
                }
                _ => None,
            }
        };

        let result = match (op, &l, &r) {
            (Op::Add, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
            (Op::Sub, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a - b)),
            (Op::Mul, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
            (Op::Div, Value::Number(_, ), Value::Number(b)) if *b == 0.0 => Err("division by zero".into()),
            (Op::Div, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a / b)),
            (Op::Mod, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a % b)),
            (Op::Pow, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a.powf(*b))),

            (Op::Add, Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", a, b))),
            (Op::Add, Value::Str(a), Value::Number(b)) => Ok(Value::Str(format!("{}{}", a, format_number(*b)))),
            (Op::Add, Value::Number(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", format_number(*a), b))),
            (Op::Add, Value::Str(a), Value::Bool(b)) => Ok(Value::Str(format!("{}{}", a, b))),
            (Op::Add, Value::Bool(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", a, b))),

            (Op::Lt, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a < b)),
            (Op::Gt, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a > b)),
            (Op::Lte, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a <= b)),
            (Op::Gte, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a >= b)),

            (Op::Eq, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a == b)),
            (Op::Eq, Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a == b)),
            (Op::Eq, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a == b)),
            (Op::Eq, _, _) => Ok(Value::Bool(false)),

            (Op::Neq, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a != b)),
            (Op::Neq, Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a != b)),
            (Op::Neq, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a != b)),
            (Op::Neq, _, _) => Ok(Value::Bool(true)),

            _ => Err(format!("type error: cannot apply {:?} to {:?} and {:?}", op, type_name(&l), type_name(&r))),
        };
        Ok(retag_spice(result?, unit_after))
    }

    fn eval_call(&mut self, name: &str, args: &[Expr], depth: u32) -> Result<Value, String> {
        if depth >= MAX_CALL_DEPTH {
            return Err("maximum call depth exceeded".into());
        }

        if self.fns.contains_key(name) {
            return self.call_user_fn(name, args, depth);
        }
        if self.solved_fns.contains_key(name) {
            return self.call_solved_fn(name, args, depth);
        }

        match name {
            "sin" | "cos" | "tan" | "asin" | "acos" | "atan" |
            "sqrt" | "abs" | "ln" | "log" => {
                if args.len() != 1 {
                    return Err(format!("{}() expects 1 argument", name));
                }
                let v = self.eval_expr(&args[0], depth)?;
                let (raw, unit) = unwrap_spice(&v);
                let n = match raw {
                    Value::Number(n) => n,
                    _ => return Err(format!("{}() expects a number", name)),
                };
                let result = match name {
                    "sin" => n.sin(),
                    "cos" => n.cos(),
                    "tan" => n.tan(),
                    "asin" => n.asin(),
                    "acos" => n.acos(),
                    "atan" => n.atan(),
                    "sqrt" => n.sqrt(),
                    "abs" => n.abs(),
                    "ln" => n.ln(),
                    "log" => n.log10(),
                    _ => unreachable!(),
                };
                return Ok(retag_spice(Value::Number(result), unit));
            }
            "floor" | "ceil" | "round" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(format!("{}() expects 1 or 2 arguments", name));
                }
                let v = self.eval_expr(&args[0], depth)?;
                let (raw, unit) = unwrap_spice(&v);
                let n = match raw {
                    Value::Number(n) => n,
                    _ => return Err(format!("{}() expects a number", name)),
                };
                let digits: i32 = if args.len() == 2 {
                    let d_v = self.eval_expr(&args[1], depth)?;
                    let (d_raw, _) = unwrap_spice(&d_v);
                    match d_raw {
                        Value::Number(d) if d.is_finite() && d == d.trunc() => d as i32,
                        _ => return Err(format!("{}() second argument must be an integer", name)),
                    }
                } else {
                    0
                };
                let factor = 10f64.powi(digits);
                let scaled = n * factor;
                let result = match name {
                    "floor" => scaled.floor() / factor,
                    "ceil"  => scaled.ceil()  / factor,
                    "round" => scaled.round() / factor,
                    _ => unreachable!(),
                };
                return Ok(retag_spice(Value::Number(result), unit));
            }
            "sum" | "avg" | "min" | "max" | "count" | "std_devp" | "std_devs" => {
                if args.len() != 1 {
                    return Err(format!("{}() expects 1 argument", name));
                }
                let v = self.eval_expr(&args[0], depth)?;
                let nums = flatten_numbers(&v);
                return aggregate(name, &nums);
            }
            "len" => {
                if args.len() != 1 {
                    return Err("len() expects 1 argument".into());
                }
                let v = self.eval_expr(&args[0], depth)?;
                return match v {
                    Value::Str(s) => Ok(Value::Number(s.len() as f64)),
                    Value::Array(a) => Ok(Value::Number(a.len() as f64)),
                    _ => Err("len() expects a string or array".into()),
                };
            }
            "range" => {
                if args.len() != 2 {
                    return Err("range() expects 2 arguments".into());
                }
                let start = match self.eval_expr(&args[0], depth)? {
                    Value::Number(n) => n as i64,
                    _ => return Err("range() expects numbers".into()),
                };
                let end = match self.eval_expr(&args[1], depth)? {
                    Value::Number(n) => n as i64,
                    _ => return Err("range() expects numbers".into()),
                };
                let items: Vec<Value> = (start..end).map(|n| Value::Number(n as f64)).collect();
                if items.len() > MAX_ITERATIONS {
                    return Err(format!("range too large ({} elements)", items.len()));
                }
                return Ok(Value::Array(items));
            }
            "push" => {
                if args.len() != 2 {
                    return Err("push() expects 2 arguments (array, value)".into());
                }
                let arr = self.eval_expr(&args[0], depth)?;
                let val = self.eval_expr(&args[1], depth)?;
                return match arr {
                    Value::Array(mut a) => { a.push(val); Ok(Value::Array(a)) }
                    _ => Err("push() expects an array as first argument".into()),
                };
            }
            _ => {}
        }

        Err(format!("undefined function '{}'", name))
    }

    fn call_user_fn(&mut self, name: &str, args: &[Expr], depth: u32) -> Result<Value, String> {
        let fdef = self.fns.get(name).cloned()
            .ok_or_else(|| format!("undefined function '{}'", name))?;

        if args.len() != fdef.params.len() {
            return Err(format!("{}() expects {} arguments, got {}", name, fdef.params.len(), args.len()));
        }

        let mut arg_vals = Vec::new();
        for arg in args {
            arg_vals.push(self.eval_expr(arg, depth)?);
        }

        let saved_vars = self.vars.clone();
        let saved_types = self.var_types.clone();
        for ((pname, pty), val) in fdef.params.iter().zip(arg_vals) {
            let bound = match pty {
                Some(t) => apply_type_annotation(&val, Some(t))
                    .map_err(|e| format!("{}(): parameter '{}': {}", name, pname, e))?,
                None => val,
            };
            if let Some(t) = pty {
                self.var_types.insert(pname.clone(), t.clone());
            } else {
                self.var_types.remove(pname);
            }
            self.vars.insert(pname.clone(), bound);
        }

        let mut result = Value::Void;
        for stmt in &fdef.body {
            match self.exec_stmt(stmt, depth + 1) {
                Ok(v) => result = v,
                Err(e) if e.starts_with('\x00') => {
                    self.vars = saved_vars;
                    self.var_types = saved_types;
                    let raw = decode_return(&e);
                    return Ok(apply_fn_return_type(&fdef.return_type, raw, name)?);
                }
                Err(e) => {
                    self.vars = saved_vars;
                    self.var_types = saved_types;
                    return Err(e);
                }
            }
        }

        self.vars = saved_vars;
        self.var_types = saved_types;
        apply_fn_return_type(&fdef.return_type, result, name)
    }

    fn build_solved_fn_def(
        &self,
        source_fn: &str,
        target_var: &str,
        math_form: Option<(&[String], &[String])>,
    ) -> Result<SolvedFnDef, String> {
        let fdef = self.fns.get(source_fn)
            .ok_or_else(|| format!(
                "solve: source function '{}' is not defined", source_fn
            ))?;

        let (solve_idx, new_params) = match math_form {
            None => {
                let idx = fdef.params.iter()
                    .position(|(p, _)| p == target_var)
                    .ok_or_else(|| format!(
                        "solve: '{}' is not a parameter of '{}'",
                        target_var, source_fn
                    ))?;
                let mut np = Vec::with_capacity(fdef.params.len());
                np.push("target".to_string());
                for (i, (p, _)) in fdef.params.iter().enumerate() {
                    if i != idx { np.push(p.clone()); }
                }
                (idx, np)
            }
            Some((new_params, source_args)) => {
                if source_args.len() != fdef.params.len() {
                    return Err(format!(
                        "solve: '{}' takes {} argument(s), got {} in where clause",
                        source_fn, fdef.params.len(), source_args.len()
                    ));
                }
                let idx = source_args.iter()
                    .position(|a| a == target_var)
                    .ok_or_else(|| format!(
                        "solve: target '{}' does not appear in where-clause arguments",
                        target_var
                    ))?;
                if source_args.iter().filter(|a| *a == target_var).count() > 1 {
                    return Err(format!(
                        "solve: target '{}' appears more than once in where-clause arguments",
                        target_var
                    ));
                }
                let expected_rest: Vec<&String> = source_args.iter()
                    .filter(|a| *a != target_var).collect();
                let got_rest: Vec<&String> = new_params.iter().skip(1).collect();
                if expected_rest != got_rest {
                    let expected: Vec<String> = expected_rest.iter().map(|s| (*s).clone()).collect();
                    return Err(format!(
                        "solve: function parameters after the result must be [{}] to match the where clause",
                        expected.join(", ")
                    ));
                }
                (idx, new_params.to_vec())
            }
        };

        Ok(SolvedFnDef {
            source_fn: source_fn.to_string(),
            solve_param_idx: solve_idx,
            new_params,
        })
    }

    fn call_solved_fn(&mut self, name: &str, args: &[Expr], depth: u32) -> Result<Value, String> {
        let def = self.solved_fns.get(name).cloned()
            .ok_or_else(|| format!("undefined function '{}'", name))?;
        if args.len() != def.new_params.len() {
            return Err(format!(
                "{}() expects {} arguments, got {}",
                name, def.new_params.len(), args.len()
            ));
        }
        let mut arg_vals = Vec::with_capacity(args.len());
        for a in args {
            arg_vals.push(self.eval_expr(a, depth)?);
        }
        let (target_val, _) = unwrap_spice(&arg_vals[0]);
        let target = match target_val {
            Value::Number(n) => n,
            other => return Err(format!(
                "{}() target must be a number, got {}",
                name, other.display()
            )),
        };
        let fixed: Vec<f64> = arg_vals.iter().skip(1)
            .map(|v| {
                let (raw, _) = unwrap_spice(v);
                match raw {
                    Value::Number(n) => Ok(n),
                    other => Err(format!(
                        "{}() fixed arguments must be numbers, got {}",
                        name, other.display()
                    )),
                }
            })
            .collect::<Result<_, _>>()?;
        let result = self.numerical_solve(&def, target, &fixed, depth)?;
        Ok(Value::Number(result))
    }

    /// runs damped Newton's method with a secant-approximated derivative.
    fn numerical_solve(
        &mut self,
        def: &SolvedFnDef,
        target: f64,
        fixed: &[f64],
        depth: u32,
    ) -> Result<f64, String> {
        const MAX_ITERS: u32 = 100;
        const EPSILON: f64 = 1e-10;
        const DERIV_EPSILON: f64 = 1e-14;
        const MIN_DAMP: f64 = 1e-10;

        let mut x: f64 = 1.0;
        for iter in 0..MAX_ITERS {
            let fx = match self.eval_source_at(def, x, fixed, depth) {
                Ok(v) if v.is_finite() => v - target,
                Ok(_) => {
                    return Err(format!(
                        "solve: '{}' produced a non-finite value at iteration {}",
                        def.source_fn, iter
                    ));
                }
                Err(e) => {
                    return Err(format!(
                        "solve: '{}' at iteration {}: {}",
                        def.source_fn, iter, e
                    ));
                }
            };
            if fx.abs() < EPSILON {
                return Ok(x);
            }
            let h = (x.abs() * 1e-6).max(1e-9);
            let fx_h = self.probe_finite(def, x + h, fixed, depth)
                .or_else(|_| self.probe_finite(def, x - h, fixed, depth).map(|v| {
                    2.0 * fx - v
                }))
                .map_err(|_| format!(
                    "solve: could not probe derivative of '{}' near x={}",
                    def.source_fn, x
                ))? - target;
            let dfx = (fx_h - fx) / h;
            if dfx.abs() < DERIV_EPSILON {
                return Err(format!(
                    "solve: '{}' has a flat derivative near x={} — nothing to invert",
                    def.source_fn, x
                ));
            }
            let step = fx / dfx;
            let mut alpha: f64 = 1.0;
            loop {
                let candidate = x - alpha * step;
                match self.eval_source_at(def, candidate, fixed, depth) {
                    Ok(v) if v.is_finite() => {
                        x = candidate;
                        break;
                    }
                    _ => {
                        alpha *= 0.5;
                        if alpha < MIN_DAMP {
                            return Err(format!(
                                "solve: '{}' — cannot find a finite step from x={}",
                                def.source_fn, x
                            ));
                        }
                    }
                }
            }
        }
        Err(format!(
            "solve: did not converge in {} iterations (source '{}')",
            MAX_ITERS, def.source_fn
        ))
    }

    fn probe_finite(
        &mut self,
        def: &SolvedFnDef,
        x: f64,
        fixed: &[f64],
        depth: u32,
    ) -> Result<f64, String> {
        let v = self.eval_source_at(def, x, fixed, depth)?;
        if v.is_finite() { Ok(v) } else { Err("non-finite".into()) }
    }

    fn eval_source_at(
        &mut self,
        def: &SolvedFnDef,
        x: f64,
        fixed: &[f64],
        depth: u32,
    ) -> Result<f64, String> {
        let arity = fixed.len() + 1;
        let mut call_args: Vec<Expr> = Vec::with_capacity(arity);
        let mut fixed_iter = fixed.iter();
        for i in 0..arity {
            if i == def.solve_param_idx {
                call_args.push(Expr::Num(x));
            } else {
                let v = fixed_iter.next()
                    .ok_or_else(|| "solve: arity mismatch between solved fn and fixed args".to_string())?;
                call_args.push(Expr::Num(*v));
            }
        }
        let v = self.call_user_fn(&def.source_fn, &call_args, depth)?;
        let (raw, _unit) = unwrap_spice(&v);
        match raw {
            Value::Number(n) => Ok(n),
            other => Err(format!(
                "solve: '{}' must return a number, got {}",
                def.source_fn, other.display()
            )),
        }
    }
}

const RETURN_PREFIX: &str = "\x00return:";

fn encode_return(val: &Value) -> String {
    match val {
        Value::Number(n) => format!("n:{}", n),
        Value::Bool(b) => format!("b:{}", b),
        Value::Str(s) => format!("s:{}", s),
        Value::Void => "v:".into(),
        Value::Array(a) if a.len() == 2 => {
            if let (Value::Number(n), Value::Str(u)) = (&a[0], &a[1]) {
                return format!("q:{}|{}", n, u);
            }
            format!("s:{}", val.display())
        }
        _ => format!("s:{}", val.display()),
    }
}

fn decode_return(encoded: &str) -> Value {
    let payload = &encoded[RETURN_PREFIX.len()..];
    if let Some(rest) = payload.strip_prefix("n:") {
        rest.parse::<f64>().map(Value::Number).unwrap_or(Value::Void)
    } else if let Some(rest) = payload.strip_prefix("b:") {
        Value::Bool(rest == "true")
    } else if let Some(rest) = payload.strip_prefix("s:") {
        Value::Str(rest.to_string())
    } else if let Some(rest) = payload.strip_prefix("q:") {
        let (n_str, u) = rest.split_once('|').unwrap_or((rest, ""));
        match n_str.parse::<f64>() {
            Ok(n) => Value::Array(vec![Value::Number(n), Value::Str(u.to_string())]),
            Err(_) => Value::Void,
        }
    } else {
        Value::Void
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Number(_) => "number",
        Value::Bool(_) => "bool",
        Value::Str(_) => "str",
        Value::Array(_) => "array",
        Value::Void => "void",
        Value::Error(_) => "error",
    }
}

fn builtin_constant(name: &str) -> Option<Value> {
    match name {
        "pi" => Some(Value::Number(std::f64::consts::PI)),
        _ => None,
    }
}

fn value_is_kind(v: &Value, kind: &str) -> bool {
    match (kind, v) {
        ("int", Value::Number(n)) => *n == n.trunc() && n.is_finite(),
        ("float", Value::Number(_)) => true,
        ("number", Value::Number(_)) => true,
        ("bool", Value::Bool(_)) => true,
        ("str", Value::Str(_)) => true,
        ("array", Value::Array(_)) => true,
        _ => false,
    }
}

fn try_cast(v: &Value, target: &str) -> Option<Value> {
    match (target, v) {
        ("int", Value::Number(n)) if *n == n.trunc() && n.is_finite() => {
            Some(Value::Number(*n))
        }
        ("float", Value::Number(_)) => Some(v.clone()),
        ("bool", Value::Bool(_)) => Some(v.clone()),
        ("str", Value::Str(_)) => Some(v.clone()),

        ("bool", Value::Number(n)) => {
            if *n == 0.0 {
                Some(Value::Bool(false))
            } else if *n == 1.0 {
                Some(Value::Bool(true))
            } else {
                None
            }
        }
        ("int", Value::Bool(b)) => Some(Value::Number(if *b { 1.0 } else { 0.0 })),
        ("float", Value::Bool(b)) => Some(Value::Number(if *b { 1.0 } else { 0.0 })),

        ("str", Value::Bool(b)) => Some(Value::Str(b.to_string())),
        ("bool", Value::Str(s)) => match s.as_str() {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },

        ("str", Value::Number(n)) => Some(Value::Str(format_number(*n))),
        ("int", Value::Str(s)) => s
            .parse::<f64>()
            .ok()
            .filter(|n| *n == n.trunc() && n.is_finite())
            .map(Value::Number),
        ("float", Value::Str(s)) => s.parse::<f64>().ok().map(Value::Number),

        _ => None,
    }
}

fn coerce_to(val: &Value, target: &str) -> Result<Value, String> {
    if !is_known_type(target) {
        return Err(format!("unknown type annotation: {}", target));
    }

    let t1 = match try_cast(val, target) {
        Some(v) => v,
        None => {
            return Err(format!(
                "cannot coerce {} {} to {}",
                type_name(val),
                val.display(),
                target
            ));
        }
    };

    if values_equal(&t1, val) {
        return Ok(t1);
    }

    let back_target = match val {
        Value::Number(_) => "float",
        Value::Bool(_) => "bool",
        Value::Str(_) => "str",
        _ => {
            return Err(format!(
                "cannot coerce {} to {}",
                type_name(val),
                target
            ));
        }
    };

    let lossy = |_| {
        format!(
            "cannot coerce {} {} to {}: lossy round-trip",
            type_name(val),
            val.display(),
            target
        )
    };

    let back = try_cast(&t1, back_target).ok_or_else(|| lossy(()))?;
    if !values_equal(val, &back) {
        return Err(lossy(()));
    }
    let t2 = try_cast(&back, target).ok_or_else(|| lossy(()))?;
    if !values_equal(&t1, &t2) {
        return Err(lossy(()));
    }
    Ok(t1)
}

fn is_known_type(t: &str) -> bool {
    matches!(t, "int" | "float" | "bool" | "str")
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        _ => false,
    }
}

fn apply_type_annotation(val: &Value, ann: Option<&str>) -> Result<Value, String> {
    match ann {
        Some(a) if is_known_type(a) => coerce_to(val, a),
        Some(a) => apply_unit_annotation(val, a),
        None => Ok(val.clone()),
    }
}

fn apply_unit_annotation(val: &Value, unit: &str) -> Result<Value, String> {
    let (raw, _existing_unit) = unwrap_spice(val);
    match raw {
        Value::Number(_) => Ok(Value::Array(vec![raw, Value::Str(unit.to_string())])),
        _ => Err(format!(
            "cannot apply unit '{}' to {} {}",
            unit,
            type_name(val),
            val.display()
        )),
    }
}

fn apply_fn_return_type(ret_ty: &Option<String>, val: Value, fn_name: &str) -> Result<Value, String> {
    match ret_ty {
        Some(t) if !matches!(val, Value::Void) => apply_type_annotation(&val, Some(t))
            .map_err(|e| format!("{}() return: {}", fn_name, e)),
        _ => Ok(val),
    }
}

// --- Public API for eval.rs integration ---

pub struct InterpResult {
    pub line: usize,
    pub value: Option<Value>,
    pub format: EvalFormat,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EvalFormat {
    Inline,
    Table,
    Tree,
}

// --- Module support ---

#[derive(Debug, Clone, Default)]
pub struct ModuleExports {
    pub vars: HashMap<String, Value>,
    pub fns: HashMap<String, FnDef>,
    pub solved_fns: HashMap<String, SolvedFnDef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    pub module: String,
    pub item: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FormulaRef {
    pub block: Option<String>,
    pub table: String,
    pub cell: (u32, u32),
}

pub struct ParsedFormula {
    ast: Expr,
}

pub fn parse_formula(text: &str) -> Result<ParsedFormula, String> {
    parse_formula_with_spice(text, false)
}

pub fn parse_formula_with_spice(text: &str, spice: bool) -> Result<ParsedFormula, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("empty formula".into());
    }
    let tokens = tokenize(trimmed, spice)?;
    let mut parser = Parser::new(tokens);
    let ast = parser.parse_expr()?;
    Ok(ParsedFormula { ast })
}

impl ParsedFormula {
    pub fn refs(&self, current_table: &str) -> Vec<FormulaRef> {
        let mut out = Vec::new();
        collect_formula_refs(&self.ast, current_table, &mut out);
        out
    }
}

fn collect_formula_refs(expr: &Expr, current_table: &str, out: &mut Vec<FormulaRef>) {
    match expr {
        Expr::CellRef { block, table, target } => {
            let tname = match table {
                Some(t) => t.clone(),
                None => current_table.to_string(),
            };
            match target {
                CellRefTarget::Cell(c, r) => {
                    out.push(FormulaRef { block: block.clone(), table: tname, cell: (*c, *r) });
                }
                CellRefTarget::Range(c0, r0, c1, r1) => {
                    let (cmin, cmax) = if c0 <= c1 { (*c0, *c1) } else { (*c1, *c0) };
                    let (rmin, rmax) = if r0 <= r1 { (*r0, *r1) } else { (*r1, *r0) };
                    for r in rmin..=rmax {
                        for c in cmin..=cmax {
                            out.push(FormulaRef {
                                block: block.clone(),
                                table: tname.clone(),
                                cell: (c, r),
                            });
                        }
                    }
                }
                CellRefTarget::Whole => {
                    out.push(FormulaRef { block: block.clone(), table: tname, cell: (0, 0) });
                }
            }
        }
        Expr::Ident(name) => {
            if !current_table.is_empty() {
                if let Some((c, r)) = parse_cell_address(name) {
                    out.push(FormulaRef {
                        block: None,
                        table: current_table.to_string(),
                        cell: (c, r),
                    });
                }
            }
        }
        Expr::BinOp(_, l, r) => {
            collect_formula_refs(l, current_table, out);
            collect_formula_refs(r, current_table, out);
        }
        Expr::UnaryOp(_, inner) => collect_formula_refs(inner, current_table, out),
        Expr::Call(_, args) => {
            for a in args {
                collect_formula_refs(a, current_table, out);
            }
        }
        Expr::Array(items) => {
            for i in items {
                collect_formula_refs(i, current_table, out);
            }
        }
        Expr::Index(target, idx) => {
            collect_formula_refs(target, current_table, out);
            collect_formula_refs(idx, current_table, out);
        }
        Expr::Range(s, e) => {
            collect_formula_refs(s, current_table, out);
            collect_formula_refs(e, current_table, out);
        }
        Expr::IsCheck(inner, _) => collect_formula_refs(inner, current_table, out),
        Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::SolveMacro { .. } => {}
    }
}

impl Interpreter {
    pub fn exports(&self) -> ModuleExports {
        ModuleExports {
            vars: self.vars.clone(),
            fns: self.fns.clone(),
            solved_fns: self.solved_fns.clone(),
        }
    }

    /// imports every binding from `exports` into the current scope.
    pub fn import_all(&mut self, exports: &ModuleExports) {
        for (name, val) in &exports.vars {
            self.vars.insert(name.clone(), val.clone());
        }
        for (name, fndef) in &exports.fns {
            self.fns.insert(name.clone(), fndef.clone());
        }
        for (name, def) in &exports.solved_fns {
            self.solved_fns.insert(name.clone(), def.clone());
        }
    }

    pub fn import_item(&mut self, exports: &ModuleExports, item: &str) -> bool {
        let mut found = false;
        if let Some(val) = exports.vars.get(item) {
            self.vars.insert(item.to_string(), val.clone());
            found = true;
        }
        if let Some(fndef) = exports.fns.get(item) {
            self.fns.insert(item.to_string(), fndef.clone());
            found = true;
        }
        if let Some(def) = exports.solved_fns.get(item) {
            self.solved_fns.insert(item.to_string(), def.clone());
            found = true;
        }
        found
    }
}

pub fn extract_use_declarations(text: &str) -> Vec<UseDecl> {
    let mut decls = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("use ") {
            continue;
        }
        let Ok(tokens) = tokenize(trimmed, false) else { continue };
        let mut parser = Parser::new(tokens);
        if let Ok(Stmt::Use(module, item)) = parser.parse_use() {
            decls.push(UseDecl { module, item });
        }
    }
    decls
}

pub fn interpret_document(lines: &[(usize, &str, bool)]) -> Vec<InterpResult> {
    let mut interp = Interpreter::new();
    interpret_document_with(&mut interp, lines)
}

pub fn interpret_document_with(interp: &mut Interpreter, lines: &[(usize, &str, bool)]) -> Vec<InterpResult> {
    let mut results = Vec::new();

    let mut brace_depth: i32 = 0;
    let mut block_acc: Vec<String> = Vec::new();

    for &(idx, content, is_eval) in lines {
        if is_eval {
            if !block_acc.is_empty() {
                let block_text = block_acc.join("\n");
                block_acc.clear();
                brace_depth = 0;
                match interp.exec_line(&block_text) {
                    Ok(_) => {}
                    Err(e) => {
                        results.push(InterpResult { line: idx, value: Some(Value::Error(e)), format: EvalFormat::Inline });
                        continue;
                    }
                }
            }

            let trimmed = content.trim();
            let (format, expr) = if let Some(rest) = trimmed.strip_prefix("/=|") {
                (EvalFormat::Table, rest.trim())
            } else if let Some(rest) = trimmed.strip_prefix("/=\\") {
                (EvalFormat::Tree, rest.trim())
            } else {
                (EvalFormat::Inline, trimmed.strip_prefix("/=").unwrap_or("").trim())
            };
            if expr.is_empty() {
                results.push(InterpResult { line: idx, value: Some(Value::Error("empty expression".into())), format });
                continue;
            }
            match interp.eval_expr_str(expr) {
                Ok(val) => results.push(InterpResult { line: idx, value: Some(val), format }),
                Err(e) => results.push(InterpResult { line: idx, value: Some(Value::Error(e)), format }),
            }
        } else {
            let trimmed = content.trim();
            let opens = trimmed.matches('{').count() as i32;
            let closes = trimmed.matches('}').count() as i32;

            if brace_depth > 0 || !block_acc.is_empty() {
                block_acc.push(trimmed.to_string());
                brace_depth += opens - closes;
                if brace_depth <= 0 {
                    let block_text = block_acc.join("\n");
                    block_acc.clear();
                    brace_depth = 0;
                    if let Err(e) = interp.exec_line(&block_text) {
                        results.push(InterpResult { line: idx, value: Some(Value::Error(e)), format: EvalFormat::Inline });
                    }
                }
            } else if opens > closes {
                block_acc.push(trimmed.to_string());
                brace_depth = opens - closes;
            } else {
                if let Err(e) = interp.exec_line(trimmed) {
                    results.push(InterpResult { line: idx, value: Some(Value::Error(e)), format: EvalFormat::Inline });
                }
            }
        }
    }

    results
}

fn flatten_numbers(v: &Value) -> Vec<f64> {
    let mut out = Vec::new();
    walk(v, &mut out);
    return out;

    fn walk(v: &Value, out: &mut Vec<f64>) {
        match v {
            Value::Number(n) => out.push(*n),
            Value::Array(items) => {
                for item in items {
                    walk(item, out);
                }
            }
            Value::Str(s) => {
                if let Ok(n) = s.trim().parse::<f64>() {
                    out.push(n);
                }
            }
            _ => {}
        }
    }
}

fn aggregate(name: &str, nums: &[f64]) -> Result<Value, String> {
    match name {
        "sum" => Ok(Value::Number(nums.iter().sum())),
        "count" => Ok(Value::Number(nums.len() as f64)),
        "avg" => {
            if nums.is_empty() {
                return Err("avg() of empty range".into());
            }
            Ok(Value::Number(nums.iter().sum::<f64>() / nums.len() as f64))
        }
        "min" => nums.iter().copied().fold(None::<f64>, |acc, n| {
            Some(match acc { Some(a) => a.min(n), None => n })
        }).map(Value::Number).ok_or_else(|| "min() of empty range".into()),
        "max" => nums.iter().copied().fold(None::<f64>, |acc, n| {
            Some(match acc { Some(a) => a.max(n), None => n })
        }).map(Value::Number).ok_or_else(|| "max() of empty range".into()),
        "std_devp" | "std_devs" => {
            let n = nums.len();
            if n == 0 {
                return Err(format!("{}() of empty range", name));
            }
            if name == "std_devs" && n < 2 {
                return Err("std_devs() needs at least 2 values".into());
            }
            let mean = nums.iter().sum::<f64>() / n as f64;
            let ss: f64 = nums.iter().map(|v| (v - mean).powi(2)).sum();
            let divisor = if name == "std_devp" { n as f64 } else { (n - 1) as f64 };
            Ok(Value::Number((ss / divisor).sqrt()))
        }
        _ => Err(format!("unknown aggregate '{}'", name)),
    }
}

// --- Display helpers for type-annotated int ---

pub fn display_value_with_type(val: &Value, type_ann: Option<&str>) -> String {
    match (val, type_ann) {
        (Value::Number(n), Some("int")) => format!("{}", *n as i64),
        _ => val.display(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(input: &str) -> String {
        let lines: Vec<&str> = input.lines().collect();
        let mut tagged: Vec<(usize, &str, bool)> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let is_eval = line.trim().starts_with("/=");
            let is_comment = line.trim().starts_with("//");
            if !is_eval && !is_comment && !line.trim().is_empty() && !line.trim().starts_with('#') {
                tagged.push((i, line, false));
            } else if is_eval {
                tagged.push((i, line, true));
            }
        }
        let results = interpret_document(&tagged);
        results.iter()
            .filter_map(|r| r.value.as_ref().map(|v| v.display()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn eval_one(input: &str) -> String {
        let mut interp = Interpreter::new();
        match interp.eval_expr_str(input) {
            Ok(v) => v.display(),
            Err(e) => format!("error: {}", e),
        }
    }

    #[test]
    fn basic_arithmetic() {
        assert_eq!(eval_one("2 + 3"), "5");
        assert_eq!(eval_one("10 - 4"), "6");
        assert_eq!(eval_one("3 * 7"), "21");
        assert_eq!(eval_one("15 / 3"), "5");
        assert_eq!(eval_one("2 ^ 10"), "1024");
        assert_eq!(eval_one("10 % 3"), "1");
    }

    #[test]
    fn string_literals() {
        assert_eq!(eval_one("\"hello\""), "hello");
        assert_eq!(eval_one("\"hello\" + \" \" + \"world\""), "hello world");
    }

    #[test]
    fn string_concatenation_mixed() {
        assert_eq!(eval_one("\"val: \" + 42"), "val: 42");
        assert_eq!(eval_one("100 + \" items\""), "100 items");
    }

    #[test]
    fn boolean_literals() {
        assert_eq!(eval_one("true"), "true");
        assert_eq!(eval_one("false"), "false");
    }

    #[test]
    fn comparison_operators() {
        assert_eq!(eval_one("1 < 2"), "true");
        assert_eq!(eval_one("2 > 3"), "false");
        assert_eq!(eval_one("5 == 5"), "true");
        assert_eq!(eval_one("5 != 3"), "true");
        assert_eq!(eval_one("3 <= 3"), "true");
        assert_eq!(eval_one("4 >= 5"), "false");
    }

    #[test]
    fn logical_operators() {
        assert_eq!(eval_one("true && false"), "false");
        assert_eq!(eval_one("true || false"), "true");
        assert_eq!(eval_one("!true"), "false");
        assert_eq!(eval_one("!false"), "true");
    }

    #[test]
    fn logical_operators_keyword_forms() {
        assert_eq!(eval_one("true and false"), "false");
        assert_eq!(eval_one("true or false"), "true");
        assert_eq!(eval_one("not true"), "false");
        assert_eq!(eval_one("not false"), "true");
    }

    #[test]
    fn pi_constant() {
        let r = eval_one("pi");
        assert!(r.starts_with("3.14159"), "expected pi, got: {}", r);
        let r2 = eval_one("pi * 2");
        assert!(r2.starts_with("6.283185"), "expected 2π, got: {}", r2);
    }

    #[test]
    fn pi_can_be_shadowed_by_let() {
        let input = "let pi = 3\n/= pi";
        assert_eq!(eval(input), "3");
    }

    #[test]
    fn strip_operator_bool_to_number() {
        assert_eq!(eval_one("~true"), "1");
        assert_eq!(eval_one("~false"), "0");
    }

    #[test]
    fn strip_operator_bridges_bool_and_number() {
        let input = "let this: bool = 0\nlet that: int = 0\n/= ~this == ~that";
        assert_eq!(eval(input), "true");
    }

    #[test]
    fn strip_operator_number_is_noop() {
        assert_eq!(eval_one("~5"), "5");
        assert_eq!(eval_one("~3.14"), "3.14");
        assert_eq!(eval_one("~-5"), "-5");
    }

    #[test]
    fn strip_operator_str_is_noop() {
        assert_eq!(eval_one("~\"hello\""), "hello");
    }

    #[test]
    fn is_keyword_basic() {
        assert_eq!(eval_one("true is bool"), "true");
        assert_eq!(eval_one("false is bool"), "true");
        assert_eq!(eval_one("\"hello\" is str"), "true");
        assert_eq!(eval_one("[1, 2, 3] is array"), "true");
        assert_eq!(eval_one("1 is int"), "true");
        assert_eq!(eval_one("1 is float"), "true");
        assert_eq!(eval_one("1.0 is int"), "true");
        assert_eq!(eval_one("1.5 is int"), "false");
        assert_eq!(eval_one("1.5 is float"), "true");
        assert_eq!(eval_one("1 is bool"), "false");
        assert_eq!(eval_one("true is int"), "false");
        assert_eq!(eval_one("\"42\" is int"), "false");
    }

    #[test]
    fn is_keyword_in_if() {
        let input = "let x: bool = 0\nlet r = false\nif (x is bool) {\n    r = true\n}\n/= r";
        assert_eq!(eval(input), "true");
    }

    #[test]
    fn is_keyword_combines_with_logic() {
        let input = "let x = 5\n/= x is int and x > 0";
        assert_eq!(eval(input), "true");
        let input2 = "let x = 5\n/= x is bool or x is int";
        assert_eq!(eval(input2), "true");
    }

    #[test]
    fn logical_operators_mixed_forms() {
        assert_eq!(eval_one("true and not false"), "true");
        assert_eq!(eval_one("(true or false) and not false"), "true");
        assert_eq!(eval_one("!(true or true)"), "false");
        assert_eq!(eval_one("not (false or false)"), "true");
    }

    #[test]
    fn arrays() {
        assert_eq!(eval_one("[1, 2, 3]"), "[1, 2, 3]");
        assert_eq!(eval_one("[1, \"two\", true]"), "[1, \"two\", true]");
        assert_eq!(eval_one("[]"), "[]");
    }

    #[test]
    fn variable_binding() {
        let input = "let x = 5\n/= x + 10";
        assert_eq!(eval(input), "15");
    }

    #[test]
    fn variable_reassignment() {
        let input = "let x = 5\nx = 10\n/= x";
        assert_eq!(eval(input), "10");
    }

    #[test]
    fn while_loop() {
        let input = "let i = 0\nlet sum = 0\nwhile (i < 10) {\n    sum = sum + i\n    i = i + 1\n}\n/= sum";
        assert_eq!(eval(input), "45");
    }

    #[test]
    fn while_loop_guard() {
        let input = "let i = 0\nwhile (true) {\n    i = i + 1\n}\n/= i";
        let result = eval(input);
        assert!(result.contains("error"), "should error on infinite loop: {}", result);
    }

    #[test]
    fn function_def_and_call() {
        let input = "fn add(a, b) {\n    a + b\n}\n/= add(3, 4)";
        assert_eq!(eval(input), "7");
    }

    #[test]
    fn function_calling_function() {
        let input = "fn double(x) {\n    x * 2\n}\nfn quad(x) {\n    double(double(x))\n}\n/= quad(5)";
        assert_eq!(eval(input), "20");
    }

    #[test]
    fn type_annotation_int_lossy_rejected() {
        let input = "let x: int = 3.7\n/= x";
        let result = eval(input);
        assert!(result.contains("error") || result.contains("lossy"), "should reject lossy: {}", result);
    }

    #[test]
    fn type_annotation_int_exact_accepted() {
        let input = "let x: int = 3.0\n/= x";
        assert_eq!(eval(input), "3");
    }

    #[test]
    fn type_stickiness_reassign_lossy_rejected() {
        let input = "let f: bool = 0\nf = 0.1\n/= f";
        let result = eval(input);
        assert!(result.contains("error") || result.contains("lossy") || result.contains("false"),
            "should reject lossy reassign: {}", result);
    }

    #[test]
    fn type_stickiness_reassign_clean_accepted() {
        let input = "let f: bool = 0\nf = 1\n/= f";
        assert_eq!(eval(input), "true");
    }

    #[test]
    fn type_stickiness_redeclare_changes_type() {
        let input = "let x: int = 3\nlet x: bool = 1\n/= x";
        assert_eq!(eval(input), "true");
    }

    #[test]
    fn type_annotation_bool_valid() {
        let input = "let x: bool = 1\n/= x";
        assert_eq!(eval(input), "true");
    }

    #[test]
    fn type_annotation_bool_zero() {
        let input = "let x: bool = 0\n/= x";
        assert_eq!(eval(input), "false");
    }

    #[test]
    fn type_annotation_bool_invalid() {
        let input = "let x: bool = 2\n/= x";
        let result = eval(input);
        assert!(result.contains("error"), "should error: {}", result);
    }

    #[test]
    fn type_annotation_str() {
        let input = "let x: str = \"hello\"\n/= x";
        assert_eq!(eval(input), "hello");
    }

    #[test]
    fn type_annotation_str_from_int_clean() {
        let input = "let x: str = 42\n/= x";
        assert_eq!(eval(input), "42");
    }

    #[test]
    fn type_annotation_str_from_float_clean() {
        let input = "let x: str = 3.14\n/= x";
        assert_eq!(eval(input), "3.14");
    }

    #[test]
    fn type_annotation_int_from_str_clean() {
        let input = "let x: int = \"42\"\n/= x";
        assert_eq!(eval(input), "42");
    }

    #[test]
    fn type_annotation_int_from_str_lossy_rejected() {
        let input = "let x: int = \"3.7\"\n/= x";
        let result = eval(input);
        assert!(result.contains("error") || result.contains("lossy"),
            "should reject: {}", result);
    }

    #[test]
    fn error_undefined_variable() {
        let result = eval("/= undefined_var");
        assert!(result.contains("error"), "should error: {}", result);
        assert!(result.contains("undefined variable"), "{}", result);
    }

    #[test]
    fn error_recovery() {
        let input = "let x = bad_var\nlet y = 5\n/= y";
        let result = eval(input);
        assert!(result.contains("5"), "should recover and eval y: {}", result);
    }

    #[test]
    fn error_undefined_function() {
        let result = eval("/= nope(1, 2)");
        assert!(result.contains("error"), "should error: {}", result);
    }

    #[test]
    fn multiple_evals() {
        let input = "let a = 3\n/= a\nlet b = 7\n/= a + b";
        assert_eq!(eval(input), "3, 10");
    }

    #[test]
    fn builtin_math_functions() {
        assert_eq!(eval_one("abs(-5)"), "5");
        assert_eq!(eval_one("floor(3.7)"), "3");
        assert_eq!(eval_one("ceil(3.2)"), "4");
        assert_eq!(eval_one("sqrt(16)"), "4");
    }

    #[test]
    fn nested_expressions() {
        assert_eq!(eval_one("(2 + 3) * (4 - 1)"), "15");
        assert_eq!(eval_one("2 * (3 + 4 * 5)"), "46");
    }

    #[test]
    fn string_variable() {
        let input = "let x = \"hello\"\nlet y = \"world\"\n/= x + \" \" + y";
        assert_eq!(eval(input), "hello world");
    }

    #[test]
    fn division_by_zero() {
        let result = eval_one("1 / 0");
        assert!(result.contains("error"), "should error on div by zero: {}", result);
    }

    #[test]
    fn len_function() {
        assert_eq!(eval_one("len(\"hello\")"), "5");
        assert_eq!(eval_one("len([1, 2, 3])"), "3");
    }

    #[test]
    fn negative_numbers() {
        assert_eq!(eval_one("-5"), "-5");
        assert_eq!(eval_one("-3 + 7"), "4");
        assert_eq!(eval_one("10 + -3"), "7");
    }

    #[test]
    fn empty_array() {
        assert_eq!(eval_one("len([])"), "0");
    }

    #[test]
    fn complex_while_with_function() {
        let input = "\
fn fib(n) {
    let a = 0
    let b = 1
    let i = 0
    while (i < n) {
        let tmp = b
        b = a + b
        a = tmp
        i = i + 1
    }
    a
}
/= fib(10)";
        assert_eq!(eval(input), "55");
    }

    #[test]
    fn if_true() {
        let input = "let x = 10\nif (x > 5) {\n    x = 100\n}\n/= x";
        assert_eq!(eval(input), "100");
    }

    #[test]
    fn if_false() {
        let input = "let x = 3\nif (x > 5) {\n    x = 100\n}\n/= x";
        assert_eq!(eval(input), "3");
    }

    #[test]
    fn if_else() {
        let input = "let x = 3\nif (x > 5) {\n    x = 100\n} else {\n    x = 0\n}\n/= x";
        assert_eq!(eval(input), "0");
    }

    #[test]
    fn if_else_chain() {
        let input = "\
let x = 5
let r = 0
if (x > 10) {
    r = 3
} else if (x > 3) {
    r = 2
} else {
    r = 1
}
/= r";
        assert_eq!(eval(input), "2");
    }

    #[test]
    fn if_without_parens() {
        let input = "let x = 10\nif x > 5 {\n    x = 100\n}\n/= x";
        assert_eq!(eval(input), "100");
    }

    #[test]
    fn for_loop_array() {
        let input = "let sum = 0\nfor x in [1, 2, 3, 4, 5] {\n    sum = sum + x\n}\n/= sum";
        assert_eq!(eval(input), "15");
    }

    #[test]
    fn for_loop_range() {
        let input = "let sum = 0\nfor i in 0..5 {\n    sum = sum + i\n}\n/= sum";
        assert_eq!(eval(input), "10");
    }

    #[test]
    fn for_loop_range_fn() {
        let input = "let sum = 0\nfor i in range(1, 6) {\n    sum = sum + i\n}\n/= sum";
        assert_eq!(eval(input), "15");
    }

    #[test]
    fn array_index() {
        assert_eq!(eval_one("[10, 20, 30][1]"), "20");
    }

    #[test]
    fn array_index_variable() {
        let input = "let arr = [10, 20, 30]\n/= arr[2]";
        assert_eq!(eval(input), "30");
    }

    #[test]
    fn array_negative_index() {
        assert_eq!(eval_one("[10, 20, 30][-1]"), "30");
    }

    #[test]
    fn string_index() {
        assert_eq!(eval_one("\"hello\"[0]"), "h");
    }

    #[test]
    fn array_index_out_of_bounds() {
        let result = eval_one("[1, 2][5]");
        assert!(result.contains("error"), "should error: {}", result);
    }

    #[test]
    fn return_from_function() {
        let input = "\
fn first_positive(a, b) {
    if (a > 0) {
        return a
    }
    if (b > 0) {
        return b
    }
    return 0
}
/= first_positive(-1, 5)";
        assert_eq!(eval(input), "5");
    }

    #[test]
    fn return_early_from_loop() {
        let input = "\
fn find(arr, target) {
    for x in arr {
        if (x == target) {
            return x
        }
    }
    return -1
}
/= find([1, 2, 3, 4], 3)";
        assert_eq!(eval(input), "3");
    }

    #[test]
    fn push_builtin() {
        let input = "let arr = [1, 2]\nlet arr = push(arr, 3)\n/= arr";
        assert_eq!(eval(input), "[1, 2, 3]");
    }

    #[test]
    fn range_expression() {
        assert_eq!(eval_one("0..5"), "[0, 1, 2, 3, 4]");
    }

    #[test]
    fn use_statement_parses() {
        let mut interp = Interpreter::new();
        assert!(interp.exec_line("use budget").is_ok());
        assert!(interp.exec_line("use budget::ramp").is_ok());
    }

    #[test]
    fn extract_use_decls() {
        let text = "let x = 5\nuse calculations\nSome prose\nuse budget::ramp\n/= x";
        let decls = extract_use_declarations(text);
        assert_eq!(decls.len(), 2);
        assert_eq!(decls[0].module, "calculations");
        assert_eq!(decls[0].item, None);
        assert_eq!(decls[1].module, "budget");
        assert_eq!(decls[1].item, Some("ramp".to_string()));
    }

    #[test]
    fn extract_use_skips_invalid() {
        let text = "use\nuse 123\nuse valid_module";
        let decls = extract_use_declarations(text);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].module, "valid_module");
    }

    #[test]
    fn module_exports_and_import() {
        let mut module_a = Interpreter::new();
        module_a.exec_line("let x = 42").unwrap();
        module_a.exec_line("fn double(n) {\n    n * 2\n}").unwrap();
        let exports = module_a.exports();
        assert!(exports.vars.contains_key("x"));
        assert!(exports.fns.contains_key("double"));

        let mut module_b = Interpreter::new();
        module_b.import_all(&exports);
        let val = module_b.eval_expr_str("x").unwrap();
        assert!(matches!(val, Value::Number(n) if n == 42.0));
        let val = module_b.eval_expr_str("double(5)").unwrap();
        assert!(matches!(val, Value::Number(n) if n == 10.0));
    }

    #[test]
    fn import_specific_item() {
        let mut module_a = Interpreter::new();
        module_a.exec_line("let x = 1").unwrap();
        module_a.exec_line("let y = 2").unwrap();
        let exports = module_a.exports();

        let mut module_b = Interpreter::new();
        assert!(module_b.import_item(&exports, "x"));
        assert!(module_b.eval_expr_str("x").is_ok());
        assert!(module_b.eval_expr_str("y").is_err());
    }


    #[test]
    fn cell_address_parses_A1() {
        assert_eq!(parse_cell_address("A1"), Some((0, 0)));
        assert_eq!(parse_cell_address("a1"), Some((0, 0)));
        assert_eq!(parse_cell_address("B3"), Some((1, 2)));
        assert_eq!(parse_cell_address("Z99"), Some((25, 98)));
    }

    #[test]
    fn cell_address_parses_multi_letter_cols() {
        assert_eq!(parse_cell_address("AA1"), Some((26, 0)));
        assert_eq!(parse_cell_address("AB1"), Some((27, 0)));
        assert_eq!(parse_cell_address("BA1"), Some((52, 0)));
    }

    #[test]
    fn cell_address_rejects_malformed() {
        assert_eq!(parse_cell_address(""), None);
        assert_eq!(parse_cell_address("1A"), None);
        assert_eq!(parse_cell_address("A"), None);
        assert_eq!(parse_cell_address("1"), None);
        assert_eq!(parse_cell_address("A0"), None);
        assert_eq!(parse_cell_address("A1B"), None);
    }

    #[test]
    fn display_addr_roundtrip() {
        for col in 0..60u32 {
            for row in 0..30u32 {
                let s = display_addr(col, row);
                assert_eq!(parse_cell_address(&s), Some((col, row)));
            }
        }
    }

    #[test]
    fn read_cell_number() {
        let mut i = Interpreter::new();
        i.register_table("budget", vec![
            vec!["10".into(), "20".into()],
            vec!["30".into(), "40".into()],
        ]);
        let v = i.eval_expr_str("@Budget:A1").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 10.0));
        let v = i.eval_expr_str("@Budget:B2").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 40.0));
    }

    #[test]
    fn read_cell_str() {
        let mut i = Interpreter::new();
        i.register_table("t", vec![vec!["hello".into(), "world".into()]]);
        let v = i.eval_expr_str("@t:A1").unwrap();
        assert!(matches!(v, Value::Str(ref s) if s == "hello"));
    }

    #[test]
    fn cell_arithmetic() {
        let mut i = Interpreter::new();
        i.register_table("b", vec![vec!["10".into(), "20".into()]]);
        let v = i.eval_expr_str("@b:A1 + @b:B1").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 30.0));
    }

    #[test]
    fn cell_ref_unknown_table_errors() {
        let mut i = Interpreter::new();
        assert!(i.eval_expr_str("@Nope:A1").is_err());
    }

    #[test]
    fn cell_ref_out_of_bounds_errors() {
        let mut i = Interpreter::new();
        i.register_table("t", vec![vec!["1".into()]]);
        assert!(i.eval_expr_str("@t:Z99").is_err());
    }

    #[test]
    fn whole_table_snapshot() {
        let mut i = Interpreter::new();
        i.register_table("b", vec![vec!["1".into(), "2".into()], vec!["3".into(), "4".into()]]);
        let v = i.eval_expr_str("@b").unwrap();
        let outer = match v { Value::Array(a) => a, _ => panic!("not array") };
        assert_eq!(outer.len(), 2);
        let first = match &outer[0] { Value::Array(a) => a, _ => panic!("not array") };
        assert_eq!(first.len(), 2);
        assert!(matches!(first[0], Value::Number(n) if n == 1.0));
    }

    #[test]
    fn cross_block_qualified_ref() {
        let mut i = Interpreter::new();
        i.register_table("second::local", vec![vec!["7".into()]]);
        let v = i.eval_expr_str("@second::local:A1").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 7.0));
    }

    #[test]
    fn bare_ref_uses_current_block() {
        let mut i = Interpreter::new();
        i.register_table("second::local", vec![vec!["7".into()]]);
        i.set_current_block(Some("second"));
        let v = i.eval_expr_str("@local:A1").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 7.0));
    }

    #[test]
    fn bare_cell_ref_inside_cell_formula() {
        let mut i = Interpreter::new();
        i.register_table("budget", vec![vec!["10".into(), "20".into()]]);
        i.set_current_table(Some("budget"));
        let f = parse_formula("A1 + B1").unwrap();
        let v = i.eval_formula(&f).unwrap();
        assert!(matches!(v, Value::Number(n) if n == 30.0));
    }

    #[test]
    fn range_ref_returns_2d_array() {
        let mut i = Interpreter::new();
        i.register_table("b", vec![
            vec!["1".into(), "2".into(), "3".into()],
            vec!["4".into(), "5".into(), "6".into()],
            vec!["7".into(), "8".into(), "9".into()],
        ]);
        let v = i.eval_expr_str("@b:A1:B2").unwrap();
        let outer = match v { Value::Array(a) => a, _ => panic!() };
        assert_eq!(outer.len(), 2);
        let row0 = match &outer[0] { Value::Array(a) => a, _ => panic!() };
        assert_eq!(row0.len(), 2);
        assert!(matches!(row0[0], Value::Number(n) if n == 1.0));
        assert!(matches!(row0[1], Value::Number(n) if n == 2.0));
        let row1 = match &outer[1] { Value::Array(a) => a, _ => panic!() };
        assert!(matches!(row1[0], Value::Number(n) if n == 4.0));
    }

    #[test]
    fn range_bracket_syntax() {
        let mut i = Interpreter::new();
        i.register_table("b", vec![
            vec!["1".into(), "2".into()],
            vec!["3".into(), "4".into()],
        ]);
        let v = i.eval_expr_str("@b[A1:B2]").unwrap();
        let outer = match v { Value::Array(a) => a, _ => panic!() };
        assert_eq!(outer.len(), 2);
    }

    #[test]
    fn cell_assign_mutates_table() {
        let mut i = Interpreter::new();
        i.register_table("b", vec![vec!["0".into(), "0".into()]]);
        i.exec_line("@b:A1 = 42").unwrap();
        let v = i.eval_expr_str("@b:A1").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 42.0));
    }

    #[test]
    fn cell_assign_logs_write() {
        let mut i = Interpreter::new();
        i.register_table("b", vec![vec!["0".into()]]);
        i.exec_line("@b:A1 = 99").unwrap();
        let writes = i.drain_table_writes();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].table_key, "b");
        assert_eq!(writes[0].cell, (0, 0));
        assert_eq!(writes[0].value, "99");
    }

    #[test]
    fn cell_assign_drain_is_idempotent() {
        let mut i = Interpreter::new();
        i.register_table("b", vec![vec!["0".into()]]);
        i.exec_line("@b:A1 = 1").unwrap();
        let first = i.drain_table_writes();
        assert_eq!(first.len(), 1);
        let second = i.drain_table_writes();
        assert!(second.is_empty());
    }

    #[test]
    fn cell_assign_rejects_whole_table_target() {
        let mut i = Interpreter::new();
        i.register_table("b", vec![vec!["0".into()]]);
        assert!(i.exec_line("@b = 1").is_err());
    }

    #[test]
    fn cell_assign_rejects_range_target() {
        let mut i = Interpreter::new();
        i.register_table("b", vec![vec!["0".into(), "0".into()]]);
        assert!(i.exec_line("@b:A1:B1 = 1").is_err());
    }

    #[test]
    fn formula_refs_simple() {
        let f = parse_formula("@budget:A1 + @budget:B2").unwrap();
        let refs = f.refs("");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].table, "budget");
        assert_eq!(refs[0].cell, (0, 0));
        assert_eq!(refs[1].table, "budget");
        assert_eq!(refs[1].cell, (1, 1));
    }

    #[test]
    fn formula_refs_bare_with_current_table() {
        let f = parse_formula("A1 + B2").unwrap();
        let refs = f.refs("budget");
        assert_eq!(refs.len(), 2);
        assert!(refs.iter().all(|r| r.table == "budget" && r.block.is_none()));
    }

    #[test]
    fn formula_refs_range_expands() {
        let f = parse_formula("@t:A1:B2").unwrap();
        let refs = f.refs("");
        assert_eq!(refs.len(), 4);
    }

    #[test]
    fn formula_refs_cross_block() {
        let f = parse_formula("@second::local:A1").unwrap();
        let refs = f.refs("");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].block.as_deref(), Some("second"));
        assert_eq!(refs[0].table, "local");
    }

    #[test]
    fn cell_address_case_insensitive_parse() {
        let mut i = Interpreter::new();
        i.register_table("budget", vec![vec!["7".into()]]);
        let v = i.eval_expr_str("@BUDGET:a1").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 7.0));
    }


    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn sum_on_literal_array() {
        let mut i = Interpreter::new();
        let v = i.eval_expr_str("sum([1, 2, 3, 4])").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 10.0));
    }

    #[test]
    fn sum_on_range() {
        let mut i = Interpreter::new();
        i.register_table("t", vec![
            vec!["1".into(), "2".into()],
            vec!["3".into(), "4".into()],
        ]);
        let v = i.eval_expr_str("sum(@t:A1:B2)").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 10.0));
    }

    #[test]
    fn sum_skips_non_numeric() {
        let mut i = Interpreter::new();
        i.register_table("t", vec![
            vec!["label".into(), "3".into()],
            vec!["10".into(), "hello".into()],
        ]);
        let v = i.eval_expr_str("sum(@t)").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 13.0));
    }

    #[test]
    fn avg_basic() {
        let mut i = Interpreter::new();
        let v = i.eval_expr_str("avg([2, 4, 6])").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 4.0));
    }

    #[test]
    fn avg_on_empty_errors() {
        let mut i = Interpreter::new();
        assert!(i.eval_expr_str("avg([])").is_err());
    }

    #[test]
    fn min_and_max() {
        let mut i = Interpreter::new();
        let v = i.eval_expr_str("min([5, 2, 8, 1, 9])").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 1.0));
        let v = i.eval_expr_str("max([5, 2, 8, 1, 9])").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 9.0));
    }

    #[test]
    fn count_on_mixed_range() {
        let mut i = Interpreter::new();
        i.register_table("t", vec![
            vec!["1".into(), "2".into(), "hello".into()],
            vec!["3".into(), "".into(),  "4".into()],
        ]);
        let v = i.eval_expr_str("count(@t)").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 4.0));
    }

    #[test]
    fn std_devp_matches_formula() {
        let mut i = Interpreter::new();
        let v = i.eval_expr_str("std_devp([2, 4, 4, 4, 5, 5, 7, 9])").unwrap();
        match v {
            Value::Number(n) => assert!(approx(n, 2.0), "got {}", n),
            _ => panic!("not a number"),
        }
    }

    #[test]
    fn std_devs_differs_from_std_devp() {
        let mut i = Interpreter::new();
        let p = match i.eval_expr_str("std_devp([1, 2, 3, 4])").unwrap() {
            Value::Number(n) => n,
            _ => panic!(),
        };
        let s = match i.eval_expr_str("std_devs([1, 2, 3, 4])").unwrap() {
            Value::Number(n) => n,
            _ => panic!(),
        };
        assert!(s > p, "sample ({}) should exceed population ({})", s, p);
    }

    #[test]
    fn std_devs_needs_two_values() {
        let mut i = Interpreter::new();
        assert!(i.eval_expr_str("std_devs([5])").is_err());
    }

    #[test]
    fn round_with_digits() {
        let mut i = Interpreter::new();
        let v = i.eval_expr_str("round(3.14159, 2)").unwrap();
        match v { Value::Number(n) => assert!(approx(n, 3.14)), _ => panic!() }
        let v = i.eval_expr_str("round(3.14159, 4)").unwrap();
        match v { Value::Number(n) => assert!(approx(n, 3.1416)), _ => panic!() }
        let v = i.eval_expr_str("round(3.7)").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 4.0));
    }

    #[test]
    fn ceil_with_digits() {
        let mut i = Interpreter::new();
        let v = i.eval_expr_str("ceil(1.234, 1)").unwrap();
        match v { Value::Number(n) => assert!(approx(n, 1.3)), _ => panic!() }
        let v = i.eval_expr_str("ceil(1.01)").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 2.0));
    }

    #[test]
    fn floor_with_digits() {
        let mut i = Interpreter::new();
        let v = i.eval_expr_str("floor(1.999, 2)").unwrap();
        match v { Value::Number(n) => assert!(approx(n, 1.99)), _ => panic!() }
        let v = i.eval_expr_str("floor(1.9)").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 1.0));
    }

    #[test]
    fn round_digits_must_be_integer() {
        let mut i = Interpreter::new();
        assert!(i.eval_expr_str("round(3.14, 1.5)").is_err());
    }

    #[test]
    fn aggregate_rejects_zero_or_many_args() {
        let mut i = Interpreter::new();
        assert!(i.eval_expr_str("sum()").is_err());
        assert!(i.eval_expr_str("avg(1, 2)").is_err());
    }

    fn solve_interp() -> Interpreter {
        let mut i = Interpreter::new();
        i.exec_line("fn square(x) { x * x }").unwrap();
        i
    }

    #[test]
    fn solve_macro_parses_comma() {
        let mut i = solve_interp();
        i.exec_line("let invsq = solve!(x, square)").unwrap();
        assert!(i.solved_fns.contains_key("invsq"));
        let def = &i.solved_fns["invsq"];
        assert_eq!(def.source_fn, "square");
        assert_eq!(def.solve_param_idx, 0);
        assert_eq!(def.new_params.len(), 1);
    }

    #[test]
    fn solve_macro_parses_from() {
        let mut i = solve_interp();
        i.exec_line("let invsq = solve!(x from square)").unwrap();
        let def = &i.solved_fns["invsq"];
        assert_eq!(def.source_fn, "square");
        assert_eq!(def.solve_param_idx, 0);
    }

    #[test]
    fn solve_macro_unknown_source_errors() {
        let mut i = Interpreter::new();
        let err = i.exec_line("let bad = solve!(x, nonexistent)").unwrap_err();
        assert!(err.contains("not defined"), "error was: {}", err);
    }

    #[test]
    fn solve_macro_unknown_var_errors() {
        let mut i = solve_interp();
        let err = i.exec_line("let bad = solve!(y, square)").unwrap_err();
        assert!(err.contains("not a parameter"), "error was: {}", err);
    }

    #[test]
    fn math_form_parses() {
        let mut i = solve_interp();
        i.exec_line("let invsq(out) = x where square(x) = out").unwrap();
        let def = &i.solved_fns["invsq"];
        assert_eq!(def.source_fn, "square");
        assert_eq!(def.solve_param_idx, 0);
        assert_eq!(def.new_params, vec!["out".to_string()]);
    }

    #[test]
    fn math_form_result_not_first_errors() {
        let mut i = solve_interp();
        let err = i.exec_line("let bad(x) = x where square(x) = out").unwrap_err();
        assert!(err.contains("first parameter"), "error was: {}", err);
    }

    #[test]
    fn math_form_mismatched_params_errors() {
        let mut i = Interpreter::new();
        i.exec_line("fn f(a, b) { a + b }").unwrap();
        let err = i.exec_line("let bad(out, c) = a where f(a, b) = out").unwrap_err();
        assert!(err.contains("parameters"), "error was: {}", err);
    }

    #[test]
    fn lc_tank_inversion() {
        let mut i = Interpreter::new();
        i.exec_line("fn f0(l, c) { 1 / (2 * pi * sqrt(l * c)) }").unwrap();
        i.exec_line("let lfreq = solve!(l, f0)").unwrap();
        let v = i.eval_expr_str("lfreq(1000000, 1 / 1000000000)").unwrap();
        let got = match v { Value::Number(n) => n, _ => panic!("not a number") };
        let pi = std::f64::consts::PI;
        let f = 1_000_000.0f64;
        let c = 1e-9;
        let want = 1.0 / (4.0 * pi * pi * f * f * c);
        assert!((got - want).abs() / want < 1e-6, "got {}, want {}", got, want);
    }

    #[test]
    fn math_form_and_macro_agree() {
        let mut i = Interpreter::new();
        i.exec_line("fn f0(l, c) { 1 / (2 * pi * sqrt(l * c)) }").unwrap();
        i.exec_line("let a = solve!(l, f0)").unwrap();
        i.exec_line("let b(freq, c) = l where f0(l, c) = freq").unwrap();
        let av = i.eval_expr_str("a(1000000, 1 / 1000000000)").unwrap();
        let bv = i.eval_expr_str("b(1000000, 1 / 1000000000)").unwrap();
        let (an, bn) = match (av, bv) {
            (Value::Number(a), Value::Number(b)) => (a, b),
            _ => panic!("not numbers"),
        };
        assert!((an - bn).abs() < 1e-9, "macro {} vs math {}", an, bn);
    }

    #[test]
    fn non_convergent_errors() {
        let mut i = Interpreter::new();
        i.exec_line("fn flat(x) { 42 }").unwrap();
        i.exec_line("let inv = solve!(x, flat)").unwrap();
        let err = i.eval_expr_str("inv(10)").unwrap_err();
        assert!(
            err.contains("flat derivative") || err.contains("did not converge"),
            "unexpected error: {}", err
        );
    }

    #[test]
    fn solve_macro_outside_let_errors() {
        let mut i = solve_interp();
        let err = i.eval_expr_str("solve!(x, square)").unwrap_err();
        assert!(err.contains("right-hand side"), "error was: {}", err);
    }

    #[test]
    fn let_with_params_is_regular_fn_def() {
        let mut i = Interpreter::new();
        i.exec_line("let double(x) = x * 2").unwrap();
        assert!(i.fns.contains_key("double"));
        let v = i.eval_expr_str("double(21)").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 42.0));
    }

    #[test]
    fn implicit_mul_number_times_ident() {
        let mut i = Interpreter::new();
        let v = i.eval_expr_str("2pi").unwrap();
        match v { Value::Number(n) => assert!(approx(n, 2.0 * std::f64::consts::PI)), _ => panic!() }
    }

    #[test]
    fn implicit_mul_number_times_paren() {
        let mut i = Interpreter::new();
        let v = i.eval_expr_str("2(3 + 4)").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 14.0));
    }

    #[test]
    fn implicit_mul_with_user_var() {
        let mut i = Interpreter::new();
        i.exec_line("let n = 2").unwrap();
        let v = i.eval_expr_str("2n").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 4.0));
    }

    #[test]
    fn implicit_mul_only_fires_adjacent() {
        let mut i = Interpreter::new();
        let v_adj = i.eval_expr_str("2pi").unwrap();
        let v_space = i.eval_expr_str("2 pi").unwrap();
        match (v_adj, v_space) {
            (Value::Number(a), Value::Number(b)) => {
                assert!(approx(a, 2.0 * std::f64::consts::PI));
                assert_eq!(b, 2.0);
            }
            _ => panic!("unexpected shapes"),
        }
    }

    #[test]
    fn scientific_notation_lowercase() {
        let mut i = Interpreter::new();
        let v = i.eval_expr_str("1e-9").unwrap();
        match v { Value::Number(n) => assert!(approx(n, 1e-9)), _ => panic!() }
    }

    #[test]
    fn scientific_notation_uppercase_and_plus() {
        let mut i = Interpreter::new();
        let v = i.eval_expr_str("2E+3").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 2000.0));
    }

    #[test]
    fn scientific_notation_negative_literal() {
        let mut i = Interpreter::new();
        let v = i.eval_expr_str("-1e3").unwrap();
        assert!(matches!(v, Value::Number(n) if n == -1000.0));
    }

    #[test]
    fn spice_off_by_default() {
        let mut i = Interpreter::new();
        assert!(i.eval_expr_str("100n").is_err());
    }

    #[test]
    fn spice_prefix_only() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("100n").unwrap();
        let (n, u) = match v {
            Value::Array(a) if a.len() == 2 => match (&a[0], &a[1]) {
                (Value::Number(n), Value::Str(u)) => (*n, u.clone()),
                _ => panic!("not spice-shaped"),
            },
            _ => panic!("not an array"),
        };
        assert!(approx(n, 1e-7));
        assert_eq!(u, "");
    }

    #[test]
    fn spice_prefix_with_unit() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("100nF").unwrap();
        assert_eq!(v.display(), "100NF");
    }

    #[test]
    fn spice_unit_only_no_prefix() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("80Hz").unwrap();
        assert_eq!(v.display(), "80HZ");
    }

    #[test]
    fn spice_micro_sign() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("10µF").unwrap();
        assert_eq!(v.display(), "10UF");
    }

    #[test]
    fn spice_arithmetic_preserves_unit() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("100nF + 1nF").unwrap();
        assert_eq!(v.display(), "101NF");
    }

    #[test]
    fn spice_cross_magnitude_renormalization() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("2500nF").unwrap();
        assert_eq!(v.display(), "2.5UF");
    }

    #[test]
    fn spice_scalar_op_preserves_unit() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("100nF * 2").unwrap();
        assert_eq!(v.display(), "200NF");
    }

    #[test]
    fn spice_unrecognized_suffix_falls_back_to_implicit_mul() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("2pi").unwrap();
        match v { Value::Number(n) => assert!(approx(n, 2.0 * std::f64::consts::PI)), _ => panic!() }
    }

    #[test]
    fn spice_display_small_value() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("0.5nF").unwrap();
        assert_eq!(v.display(), "500PF");
    }

    #[test]
    fn spice_negative_literal() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("-100nF").unwrap();
        assert_eq!(v.display(), "-100NF");
    }

    #[test]
    fn spice_plain_number_display_unchanged() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("1 + 1").unwrap();
        assert_eq!(v.display(), "2");
    }

    #[test]
    fn unit_mul_different_labels_concatenates() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        i.exec_line("let a = 2F").unwrap();
        i.exec_line("let b = 3H").unwrap();
        let v = i.eval_expr_str("a * b").unwrap();
        assert_eq!(v.display(), "6 F·H");
    }

    #[test]
    fn unit_div_cancels_to_plain_number() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("6F / 3F").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 2.0));
    }

    #[test]
    fn unit_div_different_labels() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("6F / 2H").unwrap();
        assert_eq!(v.display(), "3 F/H");
    }

    #[test]
    fn unit_add_mismatched_strips() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("1F + 2H").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 3.0));
    }

    #[test]
    fn unit_add_same_label_preserves() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        let v = i.eval_expr_str("1F + 2F").unwrap();
        assert_eq!(v.display(), "3F");
    }

    #[test]
    fn unit_annotation_on_let() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        i.exec_line("let x: F = 22n").unwrap();
        let v = i.eval_expr_str("x").unwrap();
        assert_eq!(v.display(), "22NF");
    }

    #[test]
    fn unit_annotation_overrides_rhs_unit() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        i.exec_line("let x: F = 22nH").unwrap();
        assert_eq!(i.eval_expr_str("x").unwrap().display(), "22NF");
    }

    #[test]
    fn unit_annotation_wraps_plain_number() {
        let mut i = Interpreter::new();
        i.exec_line("let x: H = 5").unwrap();
        assert_eq!(i.eval_expr_str("x").unwrap().display(), "5H");
    }

    #[test]
    fn fn_param_type_wraps_arg_on_entry() {
        let mut i = Interpreter::new();
        i.exec_line("fn f(c: F) { return c }").unwrap();
        let v = i.eval_expr_str("f(5)").unwrap();
        assert_eq!(v.display(), "5F");
    }

    #[test]
    fn fn_return_type_overrides_algebra() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        i.exec_line("fn ry(c: F, l: H) -> ohm { return l * c }").unwrap();
        let v = i.eval_expr_str("ry(2, 3)").unwrap();
        assert_eq!(v.display(), "6ohm");
    }

    #[test]
    fn fn_return_type_tags_raw_result() {
        let mut i = Interpreter::new();
        i.exec_line("fn square(x) -> V { x * x }").unwrap();
        let v = i.eval_expr_str("square(4)").unwrap();
        assert_eq!(v.display(), "16V");
    }

    #[test]
    fn solve_through_typed_source_fn() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        i.exec_line("fn f0(l: H, c: F) -> Hz {\n    return 1 / ((2 * pi) * (sqrt(l * c)))\n}").unwrap();
        i.exec_line("let L_solved = solve!(l, f0)").unwrap();
        let v = i.eval_expr_str("L_solved(2600, 1nF)").unwrap();
        let n = match v {
            Value::Number(n) => n,
            Value::Array(ref a) if a.len() == 2 => match &a[0] {
                Value::Number(n) => *n,
                _ => panic!(),
            },
            _ => panic!("unexpected shape"),
        };
        let pi = std::f64::consts::PI;
        let want = 1.0 / (4.0 * pi * pi * 2600.0 * 2600.0 * 1e-9);
        assert!((n - want).abs() / want < 1e-6, "got {}, want {}", n, want);
    }

    #[test]
    fn spice_lc_tank_use_case() {
        let mut i = Interpreter::new();
        i.exec_line("use spice").unwrap();
        i.exec_line("fn L(f, c) {\n    let b = (2 * pi * f)\n    return 1 / ((b*b) * c)\n}").unwrap();
        let v = i.eval_expr_str("L(2600, 1nF)").unwrap();
        let n = match v {
            Value::Number(n) => n,
            Value::Array(ref a) if a.len() == 2 => match &a[0] {
                Value::Number(n) => *n,
                _ => panic!("not numeric"),
            },
            _ => panic!("unexpected shape"),
        };
        let pi = std::f64::consts::PI;
        let want = 1.0 / (4.0 * pi * pi * 2600.0 * 2600.0 * 1e-9);
        assert!((n - want).abs() / want < 1e-6, "got {}, want {}", n, want);
    }
}
