//! Formulas of table cells: a cell that starts with `=` is computed from the others, and the
//! file keeps the formula. Tokenizer, parser, evaluator, references and cycle detection (pure).
//!
//! Columns are letters and rows are numbers counted from the first row under the header, as in a
//! spreadsheet (`C2`, `B1:B10`); `B` alone is column B of the row the formula is in. `above`,
//! `left` and `column` are ranges by position (`column` leaves the cell itself out).

use crate::lang::tr;

/// What a cell computes to.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Text(String),
}

/// Why a formula has no result.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// Something the formula does not say right: what was met, empty if it is the end.
    Syntax(String),
    /// A function or a name that does not exist.
    Unknown(String),
    /// A function given the wrong number of arguments.
    Arguments(String),
    /// A reference past the rows or the columns of the table.
    OutOfTable(String),
    /// The cell depends on itself, directly or not.
    Cycle,
    DivideByZero,
    /// A text where a number is needed.
    NotANumber(String),
    /// A function with no number to work on.
    NoNumbers(String),
    /// A range where a single value is needed.
    Range,
}

pub type Outcome = Result<Value, Error>;

impl Error {
    /// The reason, in the language of the interface.
    pub fn reason(&self) -> String {
        match self {
            Error::Syntax(met) if met.is_empty() => tr("the formula stops too soon", "la formule s'arrête trop tôt").to_string(),
            Error::Syntax(met) => format!("{} {met}", tr("not understood:", "non compris :")),
            Error::Unknown(name) => format!("{} {name}", tr("unknown function or name:", "fonction ou nom inconnu :")),
            Error::Arguments(f) => format!("{} {f}", tr("wrong number of arguments for", "mauvais nombre d'arguments pour")),
            Error::OutOfTable(r) => format!("{} {r}", tr("reference out of the table:", "référence hors du tableau :")),
            Error::Cycle => tr("the cell depends on itself", "la cellule dépend d'elle-même").to_string(),
            Error::DivideByZero => tr("division by zero", "division par zéro").to_string(),
            Error::NotANumber(t) => format!("{} “{t}”", tr("not a number:", "pas un nombre :")),
            Error::NoNumbers(f) => format!("{} {f}", tr("no number to work on in", "aucun nombre dans")),
            Error::Range => tr("a range where one value is expected", "une plage là où une valeur est attendue").to_string(),
        }
    }
}

/// A cell is a formula when its text starts with `=` and says something after it. `==` is never a formula:
/// it opens a highlight (`==text==`).
pub fn is_formula(cell: &str) -> bool {
    cell.trim_start().strip_prefix('=').is_some_and(|body| !body.trim().is_empty() && !body.starts_with('='))
}

/// The number a cell's text writes, as a person writes it: `1 234,5`, `1,234.5`, `12.50 €`, `$3`, `15%`
/// (a hundredth). With a single separator, it is the decimal one (`1,234` is 1.234; write `1 234` or
/// `1,234.00` for a thousand), as in the sort of a table.
pub fn read_number(text: &str) -> Option<f64> {
    let mut s: String = text.chars().filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\u{2009}' | '€' | '$' | '£' | '¥')).collect();
    let percent = s.ends_with('%');
    if percent {
        s.pop();
    }
    let (negative, digits) = match s.chars().next()? {
        '-' | '\u{2212}' => (true, &s[s.chars().next()?.len_utf8()..]),
        '+' => (false, &s[1..]),
        _ => (false, s.as_str()),
    };
    if !digits.chars().any(|c| c.is_ascii_digit()) || !digits.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',') {
        return None;
    }
    let (dots, commas) = (digits.matches('.').count(), digits.matches(',').count());
    // The last separator of two kinds is the decimal one; a single one is; a repeated one groups thousands.
    let decimal = match (dots, commas) {
        (d, c) if d > 0 && c > 0 => Some(if digits.rfind('.') > digits.rfind(',') { '.' } else { ',' }),
        (1, 0) => Some('.'),
        (0, 1) => Some(','),
        _ => None,
    };
    let (int, frac) = match decimal.and_then(|d| digits.rfind(d)) {
        Some(i) => (&digits[..i], &digits[i + 1..]),
        None => (digits, ""),
    };
    // Groups of three digits after the first, all with the same separator.
    if let Some(group) = int.chars().find(|c| matches!(c, '.' | ',')) {
        let parts: Vec<&str> = int.split(group).collect();
        let ok = (1..=3).contains(&parts[0].len()) && parts[1..].iter().all(|p| p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit()));
        if !ok || parts[0].bytes().any(|b| !b.is_ascii_digit()) {
            return None;
        }
    }
    if frac.contains(['.', ',']) {
        return None;
    }
    let number = format!("{}.{}", int.replace(['.', ','], ""), frac);
    let value: f64 = number.parse().ok()?;
    let value = if percent { value / 100. } else { value };
    Some(if negative { -value } else { value })
}

/// A value as it is shown: no trailing zeros, no more than nine decimals, `decimal` for the point.
pub fn show(value: &Value, decimal: char) -> String {
    match value {
        Value::Text(text) => text.clone(),
        Value::Num(x) if !x.is_finite() => "#ERR".to_string(),
        Value::Num(x) if x.abs() >= 1e15 => format!("{x:e}").replace('.', &decimal.to_string()),
        Value::Num(x) => {
            let text = format!("{:.9}", (x * 1e9).round() / 1e9);
            let text = text.trim_end_matches('0').trim_end_matches('.');
            let text = if text == "-0" { "0" } else { text };
            text.replace('.', &decimal.to_string())
        }
    }
}

// ───────────────────────── Tokens ─────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Word(String),
    Op(char),
    Cmp(Cmp),
}

fn tokens(body: &str) -> Result<Vec<Tok>, Error> {
    let chars: Vec<char> = body.chars().collect();
    let (mut toks, mut i) = (Vec::new(), 0);
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            _ if c.is_whitespace() => i += 1,
            '0'..='9' | '.' if c.is_ascii_digit() || next.is_some_and(|n| n.is_ascii_digit()) => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                toks.push(Tok::Num(text.parse().map_err(|_| Error::Syntax(text))?));
            }
            _ if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                toks.push(Tok::Word(chars[start..i].iter().collect()));
            }
            '"' => {
                let end = chars[i + 1..].iter().position(|&x| x == '"').ok_or_else(|| Error::Syntax("\"".into()))?;
                toks.push(Tok::Str(chars[i + 1..i + 1 + end].iter().collect()));
                i += end + 2;
            }
            '=' => {
                toks.push(Tok::Cmp(Cmp::Eq));
                i += if next == Some('=') { 2 } else { 1 };
            }
            '!' if next == Some('=') => {
                toks.push(Tok::Cmp(Cmp::Ne));
                i += 2;
            }
            '<' | '>' => {
                let (cmp, len) = match (c, next) {
                    ('<', Some('=')) => (Cmp::Le, 2),
                    ('<', Some('>')) => (Cmp::Ne, 2),
                    ('>', Some('=')) => (Cmp::Ge, 2),
                    ('<', _) => (Cmp::Lt, 1),
                    _ => (Cmp::Gt, 1),
                };
                toks.push(Tok::Cmp(cmp));
                i += len;
            }
            '+' | '-' | '*' | '/' | '^' | '%' | '(' | ')' | ',' | ';' | ':' => {
                toks.push(Tok::Op(c));
                i += 1;
            }
            '\u{2212}' => {
                toks.push(Tok::Op('-'));
                i += 1;
            }
            _ => return Err(Error::Syntax(c.to_string())),
        }
    }
    Ok(toks)
}

// ───────────────────────── Syntax tree ─────────────────────────

/// A cell by its column and its row: the one of the formula, or a row number (0 is the header).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ref {
    col: usize,
    row: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Named {
    Above,
    Left,
    Column,
}

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Num(f64),
    Str(String),
    Cell(Ref),
    Between(Ref, Ref),
    Named(Named),
    Neg(Box<Expr>),
    Percent(Box<Expr>),
    Bin(char, Box<Expr>, Box<Expr>),
    Compare(Cmp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

/// Column `A`, `Z`, `AA`… of a word of one to three letters.
fn column_of(word: &str) -> Option<usize> {
    (1..=3).contains(&word.len()).then_some(())?;
    word.bytes().try_fold(0usize, |n, b| b.is_ascii_alphabetic().then(|| n * 26 + (b.to_ascii_uppercase() - b'A') as usize + 1)).map(|n| n - 1)
}

/// `C2`: a column and a row number.
fn cell_of(word: &str) -> Option<Ref> {
    let split = word.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = word.split_at(split);
    Some(Ref { col: column_of(letters)?, row: Some(digits.parse().ok().filter(|_| digits.bytes().all(|b| b.is_ascii_digit()))?) })
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn eat(&mut self, op: char) -> bool {
        let found = self.peek() == Some(&Tok::Op(op));
        self.at += found as usize;
        found
    }

    fn met(&self) -> String {
        match self.peek() {
            None => String::new(),
            Some(Tok::Num(n)) => n.to_string(),
            Some(Tok::Str(s)) => format!("\"{s}\""),
            Some(Tok::Word(w)) => w.clone(),
            Some(Tok::Op(c)) => c.to_string(),
            Some(Tok::Cmp(_)) => "=".to_string(),
        }
    }

    fn compare(&mut self) -> Result<Expr, Error> {
        let mut left = self.sum()?;
        while let Some(Tok::Cmp(cmp)) = self.peek().cloned() {
            self.at += 1;
            left = Expr::Compare(cmp, Box::new(left), Box::new(self.sum()?));
        }
        Ok(left)
    }

    fn sum(&mut self) -> Result<Expr, Error> {
        let mut left = self.product()?;
        while let Some(&Tok::Op(op @ ('+' | '-'))) = self.peek() {
            self.at += 1;
            left = Expr::Bin(op, Box::new(left), Box::new(self.product()?));
        }
        Ok(left)
    }

    fn product(&mut self) -> Result<Expr, Error> {
        let mut left = self.unary()?;
        while let Some(&Tok::Op(op @ ('*' | '/' | '%'))) = self.peek() {
            self.at += 1;
            left = Expr::Bin(op, Box::new(left), Box::new(self.unary()?));
        }
        Ok(left)
    }

    /// `-x` and `+x`; the power binds tighter: `-2^2` is -4.
    fn unary(&mut self) -> Result<Expr, Error> {
        if self.eat('-') {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.eat('+') {
            return self.unary();
        }
        let base = self.postfix()?;
        Ok(if self.eat('^') { Expr::Bin('^', Box::new(base), Box::new(self.unary()?)) } else { base })
    }

    /// `15%` is a hundredth; a `%` followed by an operand is the remainder (`7 % 3`).
    fn postfix(&mut self) -> Result<Expr, Error> {
        let mut expr = self.primary()?;
        while self.peek() == Some(&Tok::Op('%')) {
            let operand = matches!(self.toks.get(self.at + 1), Some(Tok::Num(_) | Tok::Word(_) | Tok::Str(_) | Tok::Op('(' | '-' | '+')));
            if operand {
                break;
            }
            self.at += 1;
            expr = Expr::Percent(Box::new(expr));
        }
        Ok(expr)
    }

    fn primary(&mut self) -> Result<Expr, Error> {
        let Some(tok) = self.peek().cloned() else { return Err(Error::Syntax(self.met())) };
        self.at += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Op('(') => {
                let inner = self.compare()?;
                self.eat(')').then_some(inner).ok_or_else(|| Error::Syntax(self.met()))
            }
            Tok::Word(word) => {
                let lower = word.to_lowercase();
                if self.eat('(') {
                    let mut args = Vec::new();
                    if !self.eat(')') {
                        loop {
                            args.push(self.compare()?);
                            if self.eat(')') {
                                break;
                            }
                            if !(self.eat(',') || self.eat(';')) {
                                return Err(Error::Syntax(self.met()));
                            }
                        }
                    }
                    return Ok(Expr::Call(lower, args));
                }
                let named = match lower.as_str() {
                    "above" => Some(Named::Above),
                    "left" => Some(Named::Left),
                    "column" => Some(Named::Column),
                    _ => None,
                };
                if let Some(named) = named {
                    return Ok(Expr::Named(named));
                }
                let first = cell_of(&lower).or_else(|| column_of(&lower).map(|col| Ref { col, row: None })).ok_or(Error::Unknown(word))?;
                if !self.eat(':') {
                    return Ok(Expr::Cell(first));
                }
                match self.peek().cloned() {
                    Some(Tok::Word(end)) => {
                        self.at += 1;
                        let last = cell_of(&end.to_lowercase()).or_else(|| column_of(&end.to_lowercase()).map(|col| Ref { col, row: None })).ok_or(Error::Unknown(end))?;
                        Ok(Expr::Between(first, last))
                    }
                    _ => Err(Error::Syntax(self.met())),
                }
            }
            _ => {
                self.at -= 1;
                Err(Error::Syntax(self.met()))
            }
        }
    }
}

fn parse(formula: &str) -> Result<Expr, Error> {
    let body = formula.trim_start().strip_prefix('=').unwrap_or(formula);
    let mut parser = Parser { toks: tokens(body)?, at: 0 };
    let expr = parser.compare()?;
    if parser.at < parser.toks.len() {
        return Err(Error::Syntax(parser.met()));
    }
    Ok(expr)
}

// ───────────────────────── Evaluation ─────────────────────────

/// What a cell holds when something reads it.
#[derive(Clone, Debug, PartialEq)]
enum Held {
    Num(f64),
    Text(String),
    Empty,
}

/// What an expression gives: one value, or the cells of a range.
enum Item {
    One(Held),
    Many(Vec<Held>),
}

enum State {
    Todo,
    Busy,
    Done(Outcome),
}

struct Sheet<'a> {
    rows: &'a [Vec<String>],
    state: Vec<Vec<State>>,
    /// What the cells that are not formulas hold, read once: a running total reads each of them many times.
    plain: Vec<Vec<Option<Held>>>,
}

fn flag(on: bool) -> Held {
    Held::Num(on as u8 as f64)
}

impl Sheet<'_> {
    fn width(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }

    /// What cell (`r`, `c`) holds: its number, its text, or its formula's result.
    fn held(&mut self, r: usize, c: usize) -> Result<Held, Error> {
        let text = self.rows.get(r).and_then(|row| row.get(c)).map(String::as_str).unwrap_or_default();
        if is_formula(text) {
            return match self.formula(r, c)? {
                Value::Num(n) => Ok(Held::Num(n)),
                Value::Text(t) if t.is_empty() => Ok(Held::Empty),
                Value::Text(t) => Ok(Held::Text(t)),
            };
        }
        if let Some(known) = self.plain.get(r).and_then(|row| row.get(c)).and_then(Option::as_ref) {
            return Ok(known.clone());
        }
        let held = match (text.trim(), read_number(text)) {
            ("", _) => Held::Empty,
            (_, Some(n)) => Held::Num(n),
            (t, None) => Held::Text(t.to_string()),
        };
        if let Some(slot) = self.plain.get_mut(r).and_then(|row| row.get_mut(c)) {
            *slot = Some(held.clone());
        }
        Ok(held)
    }

    /// The result of the formula in (`r`, `c`), computed once; a cell that is reached again
    /// while it is being computed depends on itself.
    fn formula(&mut self, r: usize, c: usize) -> Outcome {
        match std::mem::replace(&mut self.state[r][c], State::Busy) {
            State::Busy => return Err(Error::Cycle),
            State::Done(done) => {
                self.state[r][c] = State::Done(done.clone());
                return done;
            }
            State::Todo => {}
        }
        let outcome = parse(&self.rows[r][c]).and_then(|expr| self.eval(&expr, (r, c))).and_then(|item| match self.one(item)? {
            Held::Num(n) if n.is_finite() => Ok(Value::Num(n)),
            Held::Num(_) => Err(Error::DivideByZero),
            Held::Text(t) => Ok(Value::Text(t)),
            Held::Empty => Ok(Value::Num(0.)),
        });
        self.state[r][c] = State::Done(outcome.clone());
        outcome
    }

    fn at(&self, reference: Ref, here: (usize, usize)) -> Result<(usize, usize), Error> {
        let (r, c) = (reference.row.unwrap_or(here.0), reference.col);
        if r >= self.rows.len() || c >= self.width() {
            let name = format!("{}{}", crate::logic::markdown::column_letters(c), reference.row.map_or(String::new(), |n| n.to_string()));
            return Err(Error::OutOfTable(name));
        }
        Ok((r, c))
    }

    fn range(&mut self, cells: Vec<(usize, usize)>) -> Result<Item, Error> {
        cells.into_iter().map(|(r, c)| self.held(r, c)).collect::<Result<Vec<_>, _>>().map(Item::Many)
    }

    fn eval(&mut self, expr: &Expr, here: (usize, usize)) -> Result<Item, Error> {
        let (r, c) = here;
        Ok(match expr {
            Expr::Num(n) => Item::One(Held::Num(*n)),
            Expr::Str(s) => Item::One(Held::Text(s.clone())),
            Expr::Cell(reference) => {
                let (r, c) = self.at(*reference, here)?;
                Item::One(self.held(r, c)?)
            }
            Expr::Between(a, b) => {
                let (a, b) = (self.at(*a, here)?, self.at(*b, here)?);
                let cells = (a.0.min(b.0)..=a.0.max(b.0)).flat_map(|r| (a.1.min(b.1)..=a.1.max(b.1)).map(move |c| (r, c))).collect();
                self.range(cells)?
            }
            Expr::Named(named) => {
                let cells = match named {
                    Named::Above => (1..r).map(|r| (r, c)).collect(),
                    Named::Left => (0..c).map(|c| (r, c)).collect(),
                    Named::Column => (1..self.rows.len()).filter(|&row| row != r).map(|r| (r, c)).collect(),
                };
                self.range(cells)?
            }
            Expr::Neg(inner) => {
                let item = self.eval(inner, here)?;
                Item::One(Held::Num(-self.number(item)?))
            }
            Expr::Percent(inner) => {
                let item = self.eval(inner, here)?;
                Item::One(Held::Num(self.number(item)? / 100.))
            }
            Expr::Bin(op, a, b) => {
                let (a, b) = (self.eval(a, here)?, self.eval(b, here)?);
                let (x, y) = (self.number(a)?, self.number(b)?);
                Item::One(Held::Num(match op {
                    '+' => x + y,
                    '-' => x - y,
                    '*' => x * y,
                    '^' => x.powf(y),
                    _ if y == 0. => return Err(Error::DivideByZero),
                    '/' => x / y,
                    _ => x % y,
                }))
            }
            Expr::Compare(cmp, a, b) => {
                let (a, b) = (self.eval(a, here)?, self.eval(b, here)?);
                let (a, b) = (self.one(a)?, self.one(b)?);
                let order = match (&a, &b) {
                    (Held::Text(_), _) | (_, Held::Text(_)) => {
                        let text = |h: &Held| match h {
                            Held::Text(t) => t.to_lowercase(),
                            Held::Num(n) => show(&Value::Num(*n), '.'),
                            Held::Empty => String::new(),
                        };
                        text(&a).cmp(&text(&b))
                    }
                    _ => {
                        let num = |h: &Held| if let Held::Num(n) = h { *n } else { 0. };
                        num(&a).total_cmp(&num(&b))
                    }
                };
                Item::One(flag(match cmp {
                    Cmp::Eq => order.is_eq(),
                    Cmp::Ne => order.is_ne(),
                    Cmp::Lt => order.is_lt(),
                    Cmp::Le => order.is_le(),
                    Cmp::Gt => order.is_gt(),
                    Cmp::Ge => order.is_ge(),
                }))
            }
            Expr::Call(name, args) => self.call(name, args, here)?,
        })
    }

    /// A single value: a range of one cell is that cell.
    fn one(&mut self, item: Item) -> Result<Held, Error> {
        match item {
            Item::One(held) => Ok(held),
            Item::Many(mut cells) if cells.len() == 1 => Ok(cells.remove(0)),
            Item::Many(_) => Err(Error::Range),
        }
    }

    fn number(&mut self, item: Item) -> Result<f64, Error> {
        match self.one(item)? {
            Held::Num(n) => Ok(n),
            Held::Empty => Ok(0.),
            Held::Text(t) => Err(Error::NotANumber(t)),
        }
    }

    /// The numbers of the arguments: those of the ranges, the value of the others; text and empty cells of a
    /// range are left out.
    fn numbers(&mut self, args: &[Expr], here: (usize, usize)) -> Result<Vec<f64>, Error> {
        let mut found = Vec::new();
        for arg in args {
            match self.eval(arg, here)? {
                Item::One(Held::Num(n)) => found.push(n),
                Item::One(Held::Empty) => {}
                Item::One(Held::Text(t)) => return Err(Error::NotANumber(t)),
                Item::Many(cells) => found.extend(cells.into_iter().filter_map(|h| if let Held::Num(n) = h { Some(n) } else { None })),
            }
        }
        Ok(found)
    }

    fn call(&mut self, name: &str, args: &[Expr], here: (usize, usize)) -> Result<Item, Error> {
        let arity = |ok: bool| if ok { Ok(()) } else { Err(Error::Arguments(name.to_string())) };
        let num = |n: f64| Ok(Item::One(Held::Num(n)));
        match name {
            "sum" => num(self.numbers(args, here)?.iter().sum()),
            "count" => num(self.numbers(args, here)?.len() as f64),
            "avg" | "average" | "min" | "max" => {
                let numbers = self.numbers(args, here)?;
                if numbers.is_empty() {
                    return Err(Error::NoNumbers(name.to_string()));
                }
                num(match name {
                    "min" => numbers.iter().copied().fold(f64::INFINITY, f64::min),
                    "max" => numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    _ => numbers.iter().sum::<f64>() / numbers.len() as f64,
                })
            }
            "abs" => {
                arity(args.len() == 1)?;
                let item = self.eval(&args[0], here)?;
                num(self.number(item)?.abs())
            }
            "round" => {
                arity(matches!(args.len(), 1 | 2))?;
                let item = self.eval(&args[0], here)?;
                let x = self.number(item)?;
                let digits = match args.get(1) {
                    Some(arg) => {
                        let item = self.eval(arg, here)?;
                        self.number(item)?.clamp(-15., 15.)
                    }
                    None => 0.,
                };
                let scale = 10f64.powi(digits as i32);
                num((x * scale).round() / scale)
            }
            // Only the branch taken is computed: the other may be an error, or a cycle.
            "if" => {
                arity(matches!(args.len(), 2 | 3))?;
                let item = self.eval(&args[0], here)?;
                let held = self.one(item)?;
                let on = match held {
                    Held::Num(n) => n != 0.,
                    Held::Text(t) => !t.is_empty(),
                    Held::Empty => false,
                };
                match (on, args.get(2)) {
                    (true, _) => self.eval(&args[1], here),
                    (false, Some(other)) => self.eval(other, here),
                    (false, None) => Ok(Item::One(Held::Empty)),
                }
            }
            _ => Err(Error::Unknown(name.to_string())),
        }
    }
}

/// Whether some cell of `rows` is a formula: only then is a table worth evaluating.
pub fn any(rows: &[Vec<String>]) -> bool {
    rows.iter().flatten().any(|cell| is_formula(cell))
}

/// The result of each formula of a table (`rows`: the header, then the body, without the dash line),
/// `None` for the cells that are not one.
pub fn evaluate(rows: &[Vec<String>]) -> Vec<Vec<Option<Outcome>>> {
    let mut sheet = Sheet { rows, state: rows.iter().map(|row| row.iter().map(|_| State::Todo).collect()).collect(), plain: rows.iter().map(|row| vec![None; row.len()]).collect() };
    (0..rows.len()).map(|r| (0..rows[r].len()).map(|c| is_formula(&rows[r][c]).then(|| sheet.formula(r, c))).collect()).collect()
}

/// What each cell of a table shows, as text for a spreadsheet: a formula gives its result (`#ERR` if it has none),
/// the others their text with `\|` written `|`. `rows`: the header, then the body, without the dash line.
pub fn display(rows: &[Vec<String>], decimal: char) -> Vec<Vec<String>> {
    let results = if any(rows) { evaluate(rows) } else { Vec::new() };
    rows.iter()
        .enumerate()
        .map(|(r, row)| {
            row.iter()
                .enumerate()
                .map(|(c, text)| match results.get(r).and_then(|row| row.get(c)).and_then(Option::as_ref) {
                    Some(Ok(value)) => show(value, decimal),
                    Some(Err(_)) => "#ERR".to_string(),
                    None => text.replace("\\|", "|"),
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect()
    }

    /// The result of the formula in the last cell of the last row, shown with a point.
    fn calc(rows: &[&[&str]]) -> String {
        let rows = grid(rows);
        let results = evaluate(&rows);
        match results.last().and_then(|r| r.last()).cloned().flatten().expect("a formula") {
            Ok(v) => show(&v, '.'),
            Err(e) => format!("#ERR {e:?}"),
        }
    }

    fn one(formula: &str) -> String {
        calc(&[&["h"], &[formula]])
    }

    #[test]
    fn a_highlight_is_not_a_formula() {
        for text in ["==faux==", "==a== b", "== a ==", "==", "===", "==faux", " ==x==", "<mark class=\"yellow\">x</mark>", "{==commentaire==}{>>note<<}", "abc==", "a == b", "", "="] {
            assert!(!is_formula(text), "{text:?}");
        }
        for text in ["=A1", " =1+2", "= =1", "=sum(above)", "=1==1"] {
            assert!(is_formula(text), "{text:?}");
        }
        assert_eq!(calc(&[&["h"], &["= =1"]]), "#ERR Syntax(\"=\")");
        // Highlighted cells are text: the total ignores them and nothing in them is evaluated.
        let rows = grid(&[&["h"], &["==x=="], &["=2*3"], &["=sum(above)"]]);
        let out = evaluate(&rows);
        assert_eq!(out[1][0], None);
        assert_eq!(out[3][0], Some(Ok(Value::Num(6.))));
        assert_eq!(display(&rows, '.')[1], ["==x=="]);
    }

    #[test]
    fn reads_numbers_as_people_write_them() {
        for (text, want) in [
            ("12", 12.), ("-3", -3.), ("+3", 3.), ("−3", -3.), ("2,5", 2.5), ("2.5", 2.5), ("1 234,5", 1234.5), ("1\u{a0}234,5", 1234.5), ("1,234.5", 1234.5),
            ("1.234,5", 1234.5), ("1,234,567", 1234567.), ("1.234.567", 1234567.), ("12.50 €", 12.5), ("€12", 12.), ("$3", 3.), ("$1,234.50", 1234.5), ("15%", 0.15),
            ("-12,5 %", -0.125), (" 7 ", 7.), (".5", 0.5), ("0", 0.),
        ] {
            assert_eq!(read_number(text), Some(want), "{text:?}");
        }
        for text in ["", " ", "abc", "12 kg", "1e5", "--1", "1.2.3,4", "€", "%", "2025-01-01", "1,2,3.4.5", "**5**", "٣"] {
            assert_eq!(read_number(text), None, "{text:?}");
        }
    }

    #[test]
    fn computes_arithmetic() {
        for (formula, want) in [
            ("=1+2*3", "7"), ("=(1+2)*3", "9"), ("=2^3^2", "512"), ("=-2^2", "-4"), ("=2^-1", "0.5"), ("=10/4", "2.5"), ("=7%3", "1"), ("=7 % 3", "1"),
            ("=50%", "0.5"), ("=200*15%", "30"), ("=(1+1)%", "0.02"), ("=0.1+0.2", "0.3"), ("=1/3", "0.333333333"), ("=-(-3)", "3"), ("= 4 ", "4"),
            ("=.5+.5", "1"), ("=1e", "#ERR Syntax(\"e\")"),
        ] {
            assert_eq!(one(formula), want, "{formula}");
        }
        assert_eq!(one("=1/0"), "#ERR DivideByZero");
        assert_eq!(one("=5%0"), "#ERR DivideByZero");
        assert_eq!(one("=2^1024"), "#ERR DivideByZero");
    }

    #[test]
    fn computes_with_references() {
        let t = |f: &str| calc(&[&["Item", "Qty", "Price", "Total"], &["Coffee", "3", "2,5", "1,5"], &["Cake", "2", "4 €", f]]);
        for (formula, want) in [
            ("=B*C", "8"), ("=b*c", "8"), ("=B1*C1", "7.5"), ("=B1*C2+B2", "14"), ("=sum(B1:B2)", "5"), ("=sum(B:B)", "2"), ("=sum(C1:C2)", "6.5"),
            ("=SUM(B1:B2; C1)", "7.5"), ("=sum(above)", "1.5"), ("=left", "#ERR Range"), ("=sum(left)", "6"), ("=count(left)", "2"), ("=sum(column)", "1.5"),
            ("=avg(B1:B2)", "2.5"), ("=average(B1:B2)", "2.5"), ("=min(B1:C2)", "2"), ("=max(B1:C2)", "4"), ("=round(C1/3, 2)", "0.83"), ("=round(1234.5, -2)", "1200"),
            ("=round(2.5)", "3"), ("=abs(-4)", "4"), ("=if(B>1, 10, 20)", "10"), ("=if(B>2, 10, 20)", "20"), ("=if(B>2, 10)", "0"), ("=if(A=\"cake\", 1, 2)", "1"),
            ("=if(A<>\"Cake\", 1, 2)", "2"), ("=B1:B1", "3"), ("=C1:B1", "#ERR Range"), ("=B0", "Qty"), ("=sum(B0:B2)", "5"),
        ] {
            assert_eq!(t(formula), want, "{formula}");
        }
    }

    #[test]
    fn says_what_went_wrong() {
        let t = |f: &str| calc(&[&["a", "b"], &["1", "text"], &["2", f]]);
        for (formula, want) in [
            ("=foo(1)", "#ERR Unknown(\"foo\")"), ("=total", "#ERR Unknown(\"total\")"), ("=A9", "#ERR OutOfTable(\"A9\")"), ("=Z1", "#ERR OutOfTable(\"Z1\")"),
            ("=A1:A9", "#ERR OutOfTable(\"A9\")"), ("=B1+1", "#ERR NotANumber(\"text\")"), ("=sum(B1)", "#ERR NotANumber(\"text\")"), ("=sum(B1:B2)", "#ERR Cycle"),
            ("=B2", "#ERR Cycle"), ("=avg(\"\")", "#ERR NotANumber(\"\")"), ("=min(B1:B1)", "#ERR NoNumbers(\"min\")"), ("=round()", "#ERR Arguments(\"round\")"),
            ("=1+", "#ERR Syntax(\"\")"), ("=(1", "#ERR Syntax(\"\")"), ("=1 2", "#ERR Syntax(\"2\")"), ("=1 # 2", "#ERR Syntax(\"#\")"),
            ("=\"a", "#ERR Syntax(\"\\\"\")"), ("=sum(1 2)", "#ERR Syntax(\"2\")"), ("=if(1)", "#ERR Arguments(\"if\")"), ("=)", "#ERR Syntax(\")\")"),
        ] {
            assert_eq!(t(formula), want, "{formula}");
        }
        for e in [Error::Cycle, Error::Range, Error::Unknown("x".into()), Error::NotANumber("t".into())] {
            assert!(!e.reason().is_empty());
        }
    }

    #[test]
    fn follows_formulas_and_stops_at_cycles() {
        let rows = grid(&[&["a", "b", "c"], &["=B+1", "=C+1", "1"], &["=B2", "=A2", "=A3"], &["=sum(above)", "=column", "=if(1, 5, B3)"]]);
        let out = evaluate(&rows);
        let show_all = |r: usize| out[r].iter().map(|o| o.as_ref().map_or("-".to_string(), |o| o.as_ref().map_or("#".to_string(), |v| show(v, '.')))).collect::<Vec<_>>().join(" ");
        assert_eq!(show_all(0), "- - -");
        assert_eq!(show_all(1), "3 2 -");
        // A1 = B1+1, B1 = C1+1: no cycle; A2 = B2 = A2 is one, and so is C2.
        assert_eq!(show_all(2), "# # #");
        assert_eq!(out[2][0], Some(Err(Error::Cycle)));
        // Only the branch taken is computed.
        assert_eq!(out[3][2], Some(Ok(Value::Num(5.))));
        assert!(any(&rows) && !any(&grid(&[&["=", "a=b", "x"]])));
    }

    #[test]
    fn text_results_and_empty_cells() {
        assert_eq!(calc(&[&["a"], &["=if(1, \"yes\", \"no\")"]]), "yes");
        assert_eq!(calc(&[&["a", "b"], &["", "=A+1"]]), "1");
        assert_eq!(calc(&[&["a", "b"], &["", "=A"]]), "0");
        assert_eq!(calc(&[&["a", "b"], &["", "=if(0, 1)"]]), "0");
        assert_eq!(calc(&[&["a", "b"], &["5", "=if(A>1, \"\", 1)"]]), "");
        assert_eq!(calc(&[&["a"], &["= \"é|ü\" "]]), "é|ü");
    }

    #[test]
    fn displays_a_table_for_a_spreadsheet() {
        let rows = grid(&[&["a", "b|c", "d"], &["x\\|y", "=A+1", "=1/0"], &["2,5", "=C*2", "=1,5"]]);
        let shown = display(&rows, ',');
        assert_eq!(shown[0], ["a", "b|c", "d"]);
        assert_eq!(shown[1], ["x|y", "#ERR", "#ERR"]);
        assert_eq!(shown[2], ["2,5", "#ERR", "#ERR"]);
        assert_eq!(display(&grid(&[&["a"], &["=1.5+1"]]), ','), [["a"], ["2,5"]]);
        assert_eq!(display(&grid(&[&["a", "b"]]), '.'), [["a", "b"]]);
    }

    #[test]
    fn shows_numbers() {
        let s = |x: f64, d: char| show(&Value::Num(x), d);
        assert_eq!((s(7.5, '.'), s(7.5, ','), s(-0.0, '.'), s(1e-12, '.'), s(2., '.'), s(1234567.891, ','), s(0.1 + 0.2, '.')), ("7.5".into(), "7,5".into(), "0".into(), "0".into(), "2".into(), "1234567,891".into(), "0.3".into()));
        assert_eq!((s(1e20, '.'), s(f64::NAN, '.'), s(-1e-9, '.'), s(123456789012345., '.')), ("1e20".into(), "#ERR".into(), "-0.000000001".into(), "123456789012345".into()));
    }

    #[test]
    fn a_very_wide_or_long_table_does_not_panic() {
        let big: Vec<Vec<String>> = (0..300).map(|r| (0..40).map(|c| if r > 0 && c == 0 { format!("=sum(above)+{r}") } else { (r * c).to_string() }).collect()).collect();
        let out = evaluate(&big);
        assert!(matches!(out[299][0], Some(Ok(_))));
        assert_eq!(column_of("A"), Some(0));
        assert_eq!(column_of("AA"), Some(26));
        assert_eq!(column_of("ZZZ"), Some(18277));
        assert_eq!((column_of("AAAA"), column_of("1")), (None, None));
    }
}
