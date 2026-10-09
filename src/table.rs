//! Tables CSV/TSV : lecture, édition et écriture, sans aucune dépendance à GPUI.
//!
//! Le texte décodé est gardé une seule fois (`Arc<String>`) ; on n'indexe que les
//! limites des enregistrements (12 octets chacun). Les champs sont analysés à la
//! demande. Une ligne modifiée devient « possédée » (`Vec<String>`) ; les autres
//! sont réécrites octet pour octet depuis le texte source.
#![allow(dead_code)]

use crate::tr;
use std::borrow::Cow;
use std::fmt::{self, Write as _};
use std::io::{self, Write};
use std::ops::Range;
use std::sync::Arc;

/// Taille maximale acceptée (les positions de l'index sont des `u32`).
pub const MAX_BYTES: usize = 1 << 30;
/// Taille de l'échantillon examiné pour la détection.
const SAMPLE: usize = 64 * 1024;
/// Nombre d'enregistrements examinés pour deviner le délimiteur.
const SAMPLE_RECORDS: usize = 50;
/// Seuil de vidage du tampon d'écriture.
const FLUSH: usize = 64 * 1024;

pub const DELIMITERS: [u8; 6] = *b",;\t|: ";

pub fn delimiter_label(d: u8) -> &'static str {
    match d {
        b',' => tr("Comma", "Virgule"),
        b';' => tr("Semicolon", "Point-virgule"),
        b'\t' => tr("Tab", "Tabulation"),
        b'|' => tr("Pipe", "Barre verticale"),
        b':' => tr("Colon", "Deux-points"),
        b' ' => tr("Space", "Espace"),
        _ => tr("Other", "Autre"),
    }
}

// ───────────────────────── Encodages ─────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Latin1,
    Windows1252,
}

impl Encoding {
    pub const ALL: [Encoding; 6] = [
        Encoding::Utf8,
        Encoding::Utf8Bom,
        Encoding::Utf16Le,
        Encoding::Utf16Be,
        Encoding::Latin1,
        Encoding::Windows1252,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8Bom => "UTF-8 BOM",
            Encoding::Utf16Le => "UTF-16 LE",
            Encoding::Utf16Be => "UTF-16 BE",
            Encoding::Latin1 => "ISO-8859-1",
            Encoding::Windows1252 => "Windows-1252",
        }
    }

    pub fn from_label(s: &str) -> Option<Encoding> {
        let s = s.trim();
        Self::ALL.into_iter().find(|e| e.label().eq_ignore_ascii_case(s))
    }

    fn bom(self) -> &'static [u8] {
        match self {
            Encoding::Utf8Bom => &[0xEF, 0xBB, 0xBF],
            Encoding::Utf16Le => &[0xFF, 0xFE],
            Encoding::Utf16Be => &[0xFE, 0xFF],
            _ => &[],
        }
    }

    fn is_utf16(self) -> bool {
        matches!(self, Encoding::Utf16Le | Encoding::Utf16Be)
    }
}

/// Windows-1252, octets 0x80..=0x9F. Les cinq octets non définis donnent le point
/// de code identique (U+0081…), ce qui garde l'aller-retour sans perte.
const CP1252: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
];

fn cp1252_char(b: u8) -> char {
    match b {
        0x80..=0x9F => char::from_u32(u32::from(CP1252[usize::from(b) - 0x80])).unwrap_or('\u{FFFD}'),
        _ => char::from(b),
    }
}

fn cp1252_byte(c: char) -> Option<u8> {
    let code = u32::from(c);
    if code < 0x80 || (0xA0..=0xFF).contains(&code) {
        return u8::try_from(code).ok();
    }
    let i = CP1252.iter().position(|&u| u32::from(u) == code)?;
    u8::try_from(0x80 + i).ok()
}

pub fn detect_encoding(bytes: &[u8]) -> Encoding {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Encoding::Utf8Bom
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        Encoding::Utf16Le
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        Encoding::Utf16Be
    } else if std::str::from_utf8(bytes).is_ok() {
        Encoding::Utf8
    } else {
        Encoding::Windows1252
    }
}

/// Décode sans perte : un UTF-8 valide réutilise le tampon (zéro copie).
pub fn decode(mut bytes: Vec<u8>, enc: Encoding) -> String {
    match enc {
        Encoding::Utf8 | Encoding::Utf8Bom => {
            if enc == Encoding::Utf8Bom && bytes.starts_with(enc.bom()) {
                bytes.drain(..3);
            }
            String::from_utf8(bytes)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
        }
        Encoding::Latin1 => bytes.iter().map(|&b| char::from(b)).collect(),
        Encoding::Windows1252 => bytes.iter().map(|&b| cp1252_char(b)).collect(),
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let body = bytes.strip_prefix(enc.bom()).unwrap_or(&bytes);
            let le = enc == Encoding::Utf16Le;
            let units = body.as_chunks::<2>().0.iter().map(|p| {
                if le {
                    u16::from_le_bytes([p[0], p[1]])
                } else {
                    u16::from_be_bytes([p[0], p[1]])
                }
            });
            let mut s: String = char::decode_utf16(units)
                .map(|r| r.unwrap_or('\u{FFFD}'))
                .collect();
            if body.len() % 2 == 1 {
                s.push('\u{FFFD}');
            }
            s
        }
    }
}

/// Encode `s` à la suite de `buf`, sans BOM. Erreur : premier caractère non représentable.
fn push_encoded(buf: &mut Vec<u8>, s: &str, enc: Encoding) -> Result<(), char> {
    match enc {
        Encoding::Utf8 | Encoding::Utf8Bom => buf.extend_from_slice(s.as_bytes()),
        Encoding::Utf16Le => {
            for u in s.encode_utf16() {
                buf.extend_from_slice(&u.to_le_bytes());
            }
        }
        Encoding::Utf16Be => {
            for u in s.encode_utf16() {
                buf.extend_from_slice(&u.to_be_bytes());
            }
        }
        Encoding::Latin1 => {
            for c in s.chars() {
                buf.push(u8::try_from(u32::from(c)).map_err(|_| c)?);
            }
        }
        Encoding::Windows1252 => {
            for c in s.chars() {
                buf.push(cp1252_byte(c).ok_or(c)?);
            }
        }
    }
    Ok(())
}

pub fn encode(text: &str, enc: Encoding) -> Result<Vec<u8>, char> {
    let mut out = enc.bom().to_vec();
    push_encoded(&mut out, text, enc)?;
    Ok(out)
}

// ───────────────────────── Détection ─────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Format {
    pub encoding: Encoding,
    pub delimiter: u8,
    pub header: bool,
}

/// Délimiteur le plus plausible parmi `cands`, ou `None`. Pour chaque candidat on
/// compte les champs des premiers enregistrements (guillemets respectés) ; gagne
/// celui dont un même nombre (>1) de champs revient le plus souvent (au moins
/// `min_score` fois), puis le plus grand nombre de champs, puis le plus à gauche.
fn best_delimiter(text: &str, cands: &[u8], min_score: usize) -> Option<u8> {
    let mut end = text.len().min(SAMPLE);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let sample = &text[..end];
    let mut best: Option<(usize, usize, u8)> = None;
    for &d in cands {
        let table = Table::parse(sample.to_string(), d);
        let mut hist: Vec<(usize, usize)> = Vec::new(); // (champs, enregistrements)
        for r in 0..table.rows().min(SAMPLE_RECORDS) {
            let n = table.row(r).len();
            if n < 2 {
                continue;
            }
            match hist.iter_mut().find(|h| h.0 == n) {
                Some(h) => h.1 += 1,
                None => hist.push((n, 1)),
            }
        }
        for (n, score) in hist {
            if score >= min_score && best.is_none_or(|(bs, bn, _)| (score, n) > (bs, bn)) {
                best = Some((score, n, d));
            }
        }
    }
    best.map(|b| b.2)
}

pub fn detect_delimiter(text: &str) -> u8 {
    best_delimiter(text, &DELIMITERS, 1).unwrap_or(b',')
}

fn is_numeric(s: &str) -> bool {
    let s = s.trim();
    matches!(s.as_bytes().first(), Some(b'0'..=b'9' | b'+' | b'-' | b'.'))
        && s.replace(',', ".").parse::<f64>().is_ok()
}

pub fn detect_header(table: &Table) -> bool {
    table.rows() >= 2 && !table.row(0).iter().any(|c| is_numeric(c))
}

pub fn detect(bytes: &[u8]) -> Format {
    let encoding = detect_encoding(bytes);
    let mut end = bytes.len().min(SAMPLE);
    if encoding.is_utf16() {
        end &= !1;
    }
    // Un caractère coupé en fin d'échantillon devient U+FFFD : sans effet sur la détection.
    let sample = decode(bytes[..end].to_vec(), encoding);
    let delimiter = detect_delimiter(&sample);
    let header = detect_header(&Table::parse(sample, delimiter));
    Format { encoding, delimiter, header }
}

// ───────────────────────── Champs ─────────────────────────

/// Index du guillemet fermant d'un champ dont le contenu commence en `i`
/// (`b.len()` s'il n'est jamais fermé). `""` est un guillemet échappé.
fn quote_close(b: &[u8], mut i: usize) -> usize {
    while i < b.len() {
        if b[i] == b'"' {
            if b.get(i + 1) == Some(&b'"') {
                i += 2;
                continue;
            }
            return i;
        }
        i += 1;
    }
    b.len()
}

/// Fin du champ qui commence en `i` : index de son délimiteur, ou `b.len()`.
/// Un guillemet n'ouvre un champ qu'en première position ; le reste après le
/// guillemet fermant est pris tel quel (lecture tolérante).
fn field_end(b: &[u8], i: usize, d: u8) -> usize {
    let mut j = i;
    if b.get(i) == Some(&b'"') {
        j = quote_close(b, i + 1);
        if j < b.len() {
            j += 1;
        }
    }
    while j < b.len() && b[j] != d {
        j += 1;
    }
    j
}

/// Contenu d'un champ brut : guillemets retirés, `""` devient `"`.
fn unquote(f: &str) -> Cow<'_, str> {
    let b = f.as_bytes();
    if b.first() != Some(&b'"') {
        return Cow::Borrowed(f);
    }
    let close = quote_close(b, 1);
    let inner = &f[1..close];
    let tail = if close < b.len() { &f[close + 1..] } else { "" };
    if tail.is_empty() && !inner.contains("\"\"") {
        Cow::Borrowed(inner)
    } else {
        Cow::Owned(inner.replace("\"\"", "\"") + tail)
    }
}

fn split_fields(s: &str, d: u8) -> Vec<Cow<'_, str>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        let e = field_end(b, i, d);
        out.push(unquote(&s[i..e]));
        if e >= b.len() {
            return out;
        }
        i = e + 1;
    }
}

/// Les champs `range` d'un enregistrement brut, en un seul balayage ; ceux qui manquent sont vides.
/// Lire cellule par cellule relirait tout ce qui précède chaque fois : sur une ligne de milliers de
/// colonnes, le temps serait quadratique.
fn fields_in(s: &str, d: u8, range: std::ops::Range<usize>) -> Vec<Cow<'_, str>> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(range.len());
    let (mut i, mut col, mut done) = (0, 0, false);
    loop {
        if out.len() == range.len() {
            return out;
        }
        if done {
            out.resize(range.len(), Cow::Borrowed(""));
            return out;
        }
        let e = field_end(b, i, d);
        if col >= range.start {
            out.push(unquote(&s[i..e]));
        }
        col += 1;
        if e >= b.len() {
            done = true;
        } else {
            i = e + 1;
        }
    }
}

fn nth_field(s: &str, d: u8, col: usize) -> Cow<'_, str> {
    let b = s.as_bytes();
    let mut i = 0;
    for _ in 0..col {
        let e = field_end(b, i, d);
        if e >= b.len() {
            return Cow::Borrowed("");
        }
        i = e + 1;
    }
    unquote(&s[i..field_end(b, i, d)])
}

/// Ajoute un champ, entre guillemets s'il contient le délimiteur, `"` ou un retour ligne.
fn push_field(out: &mut String, f: &str, d: u8) {
    if f.bytes().any(|c| c == d || matches!(c, b'"' | b'\n' | b'\r')) {
        out.push('"');
        for c in f.chars() {
            if c == '"' {
                out.push('"');
            }
            out.push(c);
        }
        out.push('"');
    } else {
        out.push_str(f);
    }
}

fn push_record<S: AsRef<str>>(out: &mut String, fields: impl Iterator<Item = S>, d: u8) {
    for (i, f) in fields.enumerate() {
        if i > 0 {
            out.push(char::from(d));
        }
        push_field(out, f.as_ref(), d);
    }
}

fn index(n: usize) -> u32 {
    u32::try_from(n).expect("table trop grande")
}

// ───────────────────────── Table ─────────────────────────

/// Un enregistrement : limites dans le texte source (sans fin de ligne), ou ligne possédée.
#[derive(Clone, Copy)]
enum Rec {
    Raw(u32, u32),
    Owned(u32),
}

#[derive(Clone)]
pub struct Table {
    text: Arc<String>,
    recs: Vec<Rec>,
    /// Lignes éditées ; une ligne supprimée laisse un `Vec` vide (pas de réemploi).
    owned: Vec<Vec<String>>,
    delimiter: u8,
    eol: &'static str,
    final_newline: bool,
    // ponytail: ne diminue jamais après remove_rows/clear, une colonne vide peut subsister.
    cols: usize,
}

impl Table {
    /// Une seule passe sur les octets. `text.len()` doit être <= `MAX_BYTES`.
    pub fn parse(text: String, delimiter: u8) -> Table {
        assert!(text.len() <= MAX_BYTES, "fichier trop gros");
        let b = text.as_bytes();
        let n = b.len();
        let mut recs = Vec::new();
        let (mut start, mut fields, mut cols) = (0, 1, 0);
        let (mut field_start, mut quoted) = (true, false);
        let mut eol = None;
        let mut i = 0;
        while i < n {
            let c = b[i];
            if quoted {
                if c == b'"' {
                    if b.get(i + 1) == Some(&b'"') {
                        i += 1;
                    } else {
                        quoted = false;
                    }
                }
            } else if c == b'\n' {
                let crlf = i > start && b[i - 1] == b'\r';
                eol.get_or_insert(if crlf { "\r\n" } else { "\n" });
                recs.push(Rec::Raw(index(start), index(i - usize::from(crlf))));
                cols = cols.max(fields);
                (start, fields, field_start) = (i + 1, 1, true);
                i += 1;
                continue;
            } else if c == delimiter {
                fields += 1;
                field_start = true;
                i += 1;
                continue;
            } else if c == b'"' && field_start {
                quoted = true;
            }
            field_start = false;
            i += 1;
        }
        let final_newline = n > 0 && start == n;
        if start < n {
            recs.push(Rec::Raw(index(start), index(n)));
            cols = cols.max(fields);
        }
        Table {
            text: Arc::new(text),
            recs,
            owned: Vec::new(),
            delimiter,
            eol: eol.unwrap_or("\n"),
            final_newline,
            cols,
        }
    }

    pub fn rows(&self) -> usize {
        self.recs.len()
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn delimiter(&self) -> u8 {
        self.delimiter
    }

    fn raw(&self, s: u32, e: u32) -> &str {
        &self.text[s as usize..e as usize]
    }

    pub fn cell(&self, row: usize, col: usize) -> Cow<'_, str> {
        match self.recs.get(row) {
            Some(&Rec::Raw(s, e)) => nth_field(self.raw(s, e), self.delimiter, col),
            Some(&Rec::Owned(k)) => {
                Cow::Borrowed(self.owned[k as usize].get(col).map_or("", String::as_str))
            }
            None => Cow::Borrowed(""),
        }
    }

    /// Champs de la ligne ; peut être plus court que `cols()`.
    /// Les cellules `cols` de la ligne, vides là où la ligne est plus courte : en un seul balayage,
    /// alors que `cell` colonne par colonne serait quadratique sur une ligne très large.
    pub fn cells(&self, row: usize, cols: std::ops::Range<usize>) -> Vec<Cow<'_, str>> {
        match self.recs.get(row) {
            Some(&Rec::Raw(s, e)) => fields_in(self.raw(s, e), self.delimiter, cols),
            Some(&Rec::Owned(k)) => {
                let fields = &self.owned[k as usize];
                cols.map(|c| Cow::Borrowed(fields.get(c).map_or("", String::as_str))).collect()
            }
            None => vec![Cow::Borrowed(""); cols.len()],
        }
    }

    pub fn row(&self, row: usize) -> Vec<Cow<'_, str>> {
        match self.recs.get(row) {
            Some(&Rec::Raw(s, e)) => split_fields(self.raw(s, e), self.delimiter),
            Some(&Rec::Owned(k)) => {
                self.owned[k as usize].iter().map(|f| Cow::Borrowed(f.as_str())).collect()
            }
            None => Vec::new(),
        }
    }

    /// Ligne possédée (créée, et les lignes vides manquantes ajoutées, au besoin).
    fn owned_row(&mut self, row: usize) -> &mut Vec<String> {
        while self.recs.len() <= row {
            self.recs.push(Rec::Owned(index(self.owned.len())));
            self.owned.push(Vec::new());
        }
        if let Rec::Raw(s, e) = self.recs[row] {
            let fields = split_fields(self.raw(s, e), self.delimiter)
                .into_iter()
                .map(Cow::into_owned)
                .collect();
            self.recs[row] = Rec::Owned(index(self.owned.len()));
            self.owned.push(fields);
        }
        match self.recs[row] {
            Rec::Owned(k) => &mut self.owned[k as usize],
            Rec::Raw(..) => unreachable!("ligne rendue possédée ci-dessus"),
        }
    }

    pub fn set_cell(&mut self, row: usize, col: usize, value: String) {
        let cells = self.owned_row(row);
        if cells.len() <= col {
            cells.resize(col + 1, String::new());
        }
        cells[col] = value;
        let width = cells.len();
        self.cols = self.cols.max(width);
    }

    /// Insère une ligne vide avant `at` (`at <= rows()`).
    pub fn insert_row(&mut self, at: usize) {
        let at = at.min(self.recs.len());
        self.recs.insert(at, Rec::Owned(index(self.owned.len())));
        self.owned.push(Vec::new());
    }

    pub fn remove_rows(&mut self, range: Range<usize>) {
        let end = range.end.min(self.recs.len());
        if range.start >= end {
            return;
        }
        for rec in self.recs.drain(range.start..end) {
            if let Rec::Owned(k) = rec {
                self.owned[k as usize] = Vec::new();
            }
        }
    }

    /// Écrase à partir de `(row, col)`, en ajoutant lignes et colonnes au besoin.
    pub fn paste(&mut self, row: usize, col: usize, grid: &[Vec<String>]) {
        for (i, line) in grid.iter().enumerate() {
            if line.is_empty() {
                continue;
            }
            let cells = self.owned_row(row + i);
            if cells.len() < col + line.len() {
                cells.resize(col + line.len(), String::new());
            }
            cells[col..col + line.len()].clone_from_slice(line);
            let width = cells.len();
            self.cols = self.cols.max(width);
        }
    }

    /// Vide les cellules (sans ajouter de colonnes ni de lignes).
    pub fn clear(&mut self, rows: Range<usize>, cols: Range<usize>) {
        let cols = cols.start..cols.end.min(self.cols);
        for r in rows.start..rows.end.min(self.rows()) {
            if cols.clone().all(|c| self.cell(r, c).is_empty()) {
                continue; // rien à vider : la ligne reste telle quelle sur disque
            }
            let cells = self.owned_row(r);
            for c in cols.start..cols.end.min(cells.len()) {
                cells[c].clear();
            }
        }
    }

    /// Écrit en flux : lignes intactes octet pour octet, lignes éditées sérialisées.
    pub fn write_to(&self, out: &mut impl Write, enc: Encoding) -> Result<(), WriteError> {
        let mut buf = Vec::with_capacity(FLUSH + 4096);
        buf.extend_from_slice(enc.bom());
        let mut line = String::new();
        let last = self.recs.len().saturating_sub(1);
        for (i, rec) in self.recs.iter().enumerate() {
            let text: &str = match *rec {
                Rec::Raw(s, e) => self.raw(s, e),
                Rec::Owned(k) => {
                    line.clear();
                    push_record(&mut line, self.owned[k as usize].iter(), self.delimiter);
                    &line
                }
            };
            push_encoded(&mut buf, text, enc).map_err(WriteError::Unrepresentable)?;
            // Une dernière ligne vide sans fin de ligne disparaîtrait à la relecture.
            if i < last || self.final_newline || text.is_empty() {
                push_encoded(&mut buf, self.eol, enc).map_err(WriteError::Unrepresentable)?;
            }
            if buf.len() >= FLUSH {
                out.write_all(&buf)?;
                buf.clear();
            }
        }
        out.write_all(&buf)?;
        Ok(())
    }

    /// Copie bon marché pour écrire en tâche de fond : le texte source est partagé.
    pub fn snapshot(&self) -> Table {
        self.clone()
    }

    /// Texte d'une sélection (bornes ramenées à la table). Lignes séparées par `\n`,
    /// sans `\n` final.
    pub fn text_range(&self, rows: Range<usize>, cols: Range<usize>, style: Style) -> String {
        let rows = rows.start..rows.end.min(self.rows());
        let cols = cols.start..cols.end.min(self.cols);
        if rows.is_empty() || cols.is_empty() {
            return String::new();
        }
        let grid: Vec<Vec<Cow<str>>> = rows
            .map(|r| {
                let mut fields = self.row(r);
                cols.clone()
                    .map(|c| fields.get_mut(c).map(std::mem::take).unwrap_or_default())
                    .collect()
            })
            .collect();
        render(&grid, style)
    }
}

// ───────────────────────── Écriture ─────────────────────────

pub enum WriteError {
    Io(io::Error),
    Unrepresentable(char),
}

impl From<io::Error> for WriteError {
    fn from(e: io::Error) -> Self {
        WriteError::Io(e)
    }
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteError::Io(e) => e.fmt(f),
            WriteError::Unrepresentable(c) => write!(
                f,
                "{} '{c}'",
                tr("Character not representable in this encoding:", "Caractère impossible à représenter dans cet encodage :")
            ),
        }
    }
}

impl fmt::Debug for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for WriteError {}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    /// Tabulations ; presse-papiers.
    Tsv,
    /// RFC 4180 avec le délimiteur donné.
    Csv(u8),
    /// `| a | b |` avec une ligne de tirets après la première ligne.
    Markdown,
    /// Tableau d'objets, la première ligne donne les clés.
    Json,
}

impl Style {
    pub fn extension(self) -> &'static str {
        match self {
            Style::Tsv => "tsv",
            Style::Csv(_) => "csv",
            Style::Markdown => "md",
            Style::Json => "json",
        }
    }

    pub fn from_extension(e: &str) -> Option<Style> {
        match e.to_ascii_lowercase().as_str() {
            "tsv" => Some(Style::Tsv),
            "csv" => Some(Style::Csv(b',')),
            "md" | "markdown" => Some(Style::Markdown),
            "json" => Some(Style::Json),
            _ => None,
        }
    }
}

fn render(grid: &[Vec<Cow<str>>], style: Style) -> String {
    let mut out = String::new();
    match style {
        Style::Tsv | Style::Csv(_) => {
            let d = if let Style::Csv(d) = style { d } else { b'\t' };
            for (i, row) in grid.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                push_record(&mut out, row.iter(), d);
            }
        }
        Style::Markdown => {
            for (i, row) in grid.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                out.push('|');
                for f in row {
                    out.push(' ');
                    for c in f.chars() {
                        match c {
                            '|' => out.push_str("\\|"),
                            '\r' | '\n' => out.push(' '),
                            _ => out.push(c),
                        }
                    }
                    out.push_str(" |");
                }
                if i == 0 {
                    out.push_str("\n|");
                    out.push_str(&" --- |".repeat(row.len()));
                }
            }
        }
        Style::Json => {
            let Some((head, body)) = grid.split_first() else { return "[]".into() };
            if body.is_empty() {
                return "[]".into();
            }
            out.push_str("[\n");
            for (i, row) in body.iter().enumerate() {
                out.push_str("  {");
                for (j, v) in row.iter().enumerate() {
                    if j > 0 {
                        out.push_str(", ");
                    }
                    if head[j].is_empty() {
                        json_string(&mut out, &format!("col{}", j + 1));
                    } else {
                        json_string(&mut out, &head[j]);
                    }
                    out.push_str(": ");
                    json_string(&mut out, v);
                }
                out.push_str(if i + 1 < body.len() { "},\n" } else { "}\n" });
            }
            out.push(']');
        }
    }
    out
}

fn json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Grille d'un texte collé : tabulations si présentes, sinon délimiteur détecté
/// (`,` `;` `|` sur au moins deux lignes concordantes), sinon une seule colonne.
/// Jamais de ligne finale vide issue du `\n` final.
pub fn parse_grid(text: &str) -> Vec<Vec<String>> {
    let delimiter = if text.contains('\t') {
        b'\t'
    } else {
        // Ni espace ni deux-points : coller une phrase ne doit pas la découper.
        best_delimiter(text, &DELIMITERS[..4], 2).unwrap_or(b'\t')
    };
    let table = Table::parse(text.to_string(), delimiter);
    (0..table.rows())
        .map(|r| table.row(r).into_iter().map(Cow::into_owned).collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn table(s: &str, d: u8) -> Table {
        Table::parse(s.to_string(), d)
    }

    fn written(t: &Table, enc: Encoding) -> Vec<u8> {
        let mut out = Vec::new();
        t.write_to(&mut out, enc).unwrap();
        out
    }

    fn text_of(t: &Table) -> String {
        String::from_utf8(written(t, Encoding::Utf8)).unwrap()
    }

    #[test]
    fn encoding_detection() {
        assert_eq!(detect_encoding(b"\xEF\xBB\xBFa,b"), Encoding::Utf8Bom);
        assert_eq!(detect_encoding(b"\xFF\xFEa\0"), Encoding::Utf16Le);
        assert_eq!(detect_encoding(b"\xFE\xFF\0a"), Encoding::Utf16Be);
        assert_eq!(detect_encoding("é€".as_bytes()), Encoding::Utf8);
        assert_eq!(detect_encoding(b"caf\xE9"), Encoding::Windows1252);
        assert_eq!(detect_encoding(b"prix \x80 5"), Encoding::Windows1252);
        assert_eq!(detect_encoding(b""), Encoding::Utf8);
    }

    #[test]
    fn encoding_labels() {
        for e in Encoding::ALL {
            assert_eq!(Encoding::from_label(e.label()), Some(e));
        }
        assert_eq!(Encoding::from_label("utf-8 bom"), Some(Encoding::Utf8Bom));
        assert_eq!(Encoding::from_label("klingon"), None);
    }

    #[test]
    fn roundtrip_every_encoding() {
        let text = "a;é;ü\nb;ñ;ç\n";
        for e in Encoding::ALL {
            let bytes = encode(text, e).unwrap();
            assert!(bytes.starts_with(e.bom()));
            assert_eq!(decode(bytes, e), text, "{}", e.label());
        }
        // Hors Latin1 : seulement UTF-8, UTF-16 et Windows-1252 (pour €).
        let euro = "5€ 😀";
        for e in [Encoding::Utf8, Encoding::Utf8Bom, Encoding::Utf16Le, Encoding::Utf16Be] {
            assert_eq!(decode(encode(euro, e).unwrap(), e), euro);
        }
        assert_eq!(decode(encode("5€", Encoding::Windows1252).unwrap(), Encoding::Windows1252), "5€");
        assert_eq!(encode("€", Encoding::Windows1252).unwrap(), [0x80]);
    }

    #[test]
    fn every_byte_survives_latin1_and_1252() {
        let all: Vec<u8> = (0..=255).collect();
        for e in [Encoding::Latin1, Encoding::Windows1252] {
            let s = decode(all.clone(), e);
            assert_eq!(encode(&s, e).unwrap(), all, "{}", e.label());
        }
        assert_eq!(decode(vec![0x81, 0x8D], Encoding::Windows1252), "\u{81}\u{8D}");
    }

    #[test]
    fn decode_utf8_forced_is_lossy_and_strips_bom() {
        assert_eq!(decode(b"a\xFFb".to_vec(), Encoding::Utf8), "a\u{FFFD}b");
        assert_eq!(decode(b"\xEF\xBB\xBFx".to_vec(), Encoding::Utf8Bom), "x");
        assert_eq!(decode(b"\xFF\xFEa\0".to_vec(), Encoding::Utf16Le), "a");
    }

    #[test]
    fn unrepresentable_char() {
        assert_eq!(encode("a€", Encoding::Latin1), Err('€'));
        assert_eq!(encode("日", Encoding::Windows1252), Err('日'));
        let t = table("a,€\n", b',');
        let mut out = Vec::new();
        assert!(matches!(t.write_to(&mut out, Encoding::Latin1), Err(WriteError::Unrepresentable('€'))));
    }

    #[test]
    fn delimiter_detection() {
        assert_eq!(detect_delimiter("a,b,c\n1,2,3\n"), b',');
        assert_eq!(detect_delimiter("a;b;c\n1;2;3\n"), b';');
        assert_eq!(detect_delimiter("a\tb\tc\n1\t2\t3\n"), b'\t');
        assert_eq!(detect_delimiter("a|b|c\n1|2|3\n"), b'|');
        assert_eq!(detect_delimiter("a b c\n1 2 3\n"), b' ');
        // Le délimiteur dans un champ entre guillemets ne compte pas.
        assert_eq!(detect_delimiter("n;note\nx;\"a,b,c,d\"\ny;\"e,f\"\n"), b';');
        assert_eq!(detect_delimiter("rien
special
"), b',');
        assert_eq!(detect_delimiter(""), b',');
    }

    #[test]
    fn header_detection() {
        let t = table("nom,age\nbob,3\n", b',');
        assert!(detect_header(&t));
        assert!(!detect_header(&table("bob,3\nann,4\n", b',')));
        assert!(!detect_header(&table("nom,age\n", b',')), "une seule ligne");
        assert!(!detect_header(&table("1,5;2\nx,y\n", b';')), "virgule décimale");
    }

    #[test]
    fn detect_whole_format() {
        let bytes = "nom;ville\nzoé;Orléans\n".as_bytes();
        assert_eq!(
            detect(bytes),
            Format { encoding: Encoding::Utf8, delimiter: b';', header: true }
        );
        let latin = b"nom;ville\nzo\xE9;Orl\xE9ans\n";
        assert_eq!(detect(latin).encoding, Encoding::Windows1252);
        let utf16 = encode("a\tb\n1\t2\n", Encoding::Utf16Le).unwrap();
        let f = detect(&utf16);
        assert_eq!((f.encoding, f.delimiter, f.header), (Encoding::Utf16Le, b'\t', true));
    }

    #[test]
    fn parse_quotes_newlines_and_doubled_quotes() {
        let t = table("a,\"b,c\",\"d\"\"e\"\n\"x\ny\",,z\n", b',');
        assert_eq!((t.rows(), t.cols()), (2, 3));
        assert_eq!(t.cell(0, 1), "b,c");
        assert_eq!(t.cell(0, 2), "d\"e");
        assert_eq!(t.cell(1, 0), "x\ny");
        assert_eq!(t.cell(1, 1), "");
        assert_eq!(t.cell(1, 2), "z");
        assert_eq!(t.cell(5, 5), "");
        assert_eq!(t.cell(0, 9), "");
        assert_eq!(t.row(1), vec!["x\ny", "", "z"]);
    }

    #[test]
    fn parse_edge_cases() {
        assert_eq!(table("", b',').rows(), 0);
        let t = table("a,b\n\nc\n", b',');
        assert_eq!(t.rows(), 3, "ligne vide = enregistrement");
        assert_eq!(t.row(1), vec![""]);
        assert_eq!(t.row(2).len(), 1);
        assert_eq!(t.cols(), 2);
        assert_eq!(table("a,\n", b',').row(0), vec!["a", ""]);
        // Guillemet au milieu d'un champ : littéral.
        let t = table("5\" pipe,x\nb,c\n", b',');
        assert_eq!((t.rows(), t.cell(0, 0).into_owned()), (2, "5\" pipe".to_string()));
        // Guillemet jamais fermé : jusqu'à la fin.
        assert_eq!(table("a,\"b\nc", b',').rows(), 1);
    }

    #[test]
    fn crlf_and_final_newline_are_preserved() {
        for src in ["a,b\r\nc,d\r\n", "a,b\r\nc,d", "a,b\nc,d\n", "a,b\nc,d", "\n", "a\n\n"] {
            let mut t = table(src, b',');
            assert_eq!(text_of(&t), src);
            let v = t.cell(0, 0).into_owned();
            t.set_cell(0, 0, v); // réécrit la 1re ligne, le reste intact
            assert_eq!(text_of(&t), src);
        }
        assert_eq!(table("a\r\nb\n", b',').eol, "\r\n");
        assert_eq!(table("a", b',').eol, "\n");
        assert_eq!(table("a\n", b',').rows(), 1);
    }

    #[test]
    fn untouched_rows_are_byte_identical() {
        let src = "a, \"q\" ,z\r\n  x ,\"1\"\"2\",\r\nlast,\"multi\nline\",3\r\n";
        let mut t = table(src, b',');
        t.set_cell(1, 2, "ok".into());
        assert_eq!(
            text_of(&t),
            "a, \"q\" ,z\r\n  x ,\"1\"\"2\",ok\r\nlast,\"multi\nline\",3\r\n"
        );
    }

    #[test]
    fn set_cell_quotes_when_needed() {
        let mut t = table("a,b\nc,d\n", b',');
        t.set_cell(0, 1, "x,y".into());
        t.set_cell(1, 0, "say \"hi\"\nnow".into());
        t.set_cell(1, 3, "far".into());
        assert_eq!(t.cols(), 4);
        assert_eq!(t.cell(0, 1), "x,y");
        assert_eq!(text_of(&t), "a,\"x,y\"\n\"say \"\"hi\"\"\nnow\",d,,far\n");
        // Relecture identique.
        let back = table(&text_of(&t), b',');
        assert_eq!(back.cell(1, 0), "say \"hi\"\nnow");
        assert_eq!(back.cell(0, 1), "x,y");
    }

    #[test]
    fn insert_remove_paste() {
        let mut t = table("a,b\nc,d\n", b',');
        t.insert_row(1);
        assert_eq!((t.rows(), t.cell(1, 0).into_owned()), (3, String::new()));
        assert_eq!(text_of(&t), "a,b\n\nc,d\n");
        t.insert_row(3);
        t.set_cell(3, 0, "end".into());
        t.remove_rows(1..2);
        assert_eq!(text_of(&t), "a,b\nc,d\nend\n");
        t.remove_rows(5..9); // hors limites : sans effet
        assert_eq!(t.rows(), 3);

        let grid = vec![vec!["1".to_string(), "2".into(), "3".into()], vec!["4".into()]];
        t.paste(2, 1, &grid);
        assert_eq!((t.rows(), t.cols()), (4, 4));
        assert_eq!(text_of(&t), "a,b\nc,d\nend,1,2,3\n,4\n");
        t.paste(0, 0, &[vec!["Z".to_string()]]);
        assert_eq!(t.cell(0, 0), "Z");
        assert_eq!(t.cell(0, 1), "b");
    }

    #[test]
    fn clear_cells() {
        let mut t = table("a,b,c\nd,e,f\n,,\n", b',');
        t.clear(0..2, 1..3);
        assert_eq!(text_of(&t), "a,,\nd,,\n,,\n");
        t.clear(2..3, 0..3);
        assert!(matches!(t.recs[2], Rec::Raw(..)), "ligne déjà vide : pas modifiée");
    }

    #[test]
    fn write_in_other_encodings() {
        let t = table("é;1\n", b';');
        assert_eq!(written(&t, Encoding::Latin1), b"\xE9;1\n");
        assert_eq!(written(&t, Encoding::Utf8Bom), b"\xEF\xBB\xBF\xC3\xA9;1\n");
        assert_eq!(
            written(&t, Encoding::Utf16Be),
            [0xFE, 0xFF, 0, 0xE9, 0, b';', 0, b'1', 0, b'\n']
        );
        assert_eq!(decode(written(&t, Encoding::Utf16Le), Encoding::Utf16Le), "é;1\n");
    }

    #[test]
    fn trailing_empty_row_survives() {
        let mut t = table("a,b", b',');
        t.insert_row(1);
        assert_eq!(table(&text_of(&t), b',').rows(), 2);
    }

    #[test]
    fn snapshot_is_independent() {
        let mut t = table("a,b\nc,d\n", b',');
        let snap = t.snapshot();
        t.set_cell(0, 0, "X".into());
        assert!(Arc::ptr_eq(&t.text, &snap.text));
        assert_eq!(text_of(&snap), "a,b\nc,d\n");
        assert_eq!(text_of(&t), "X,b\nc,d\n");
    }

    #[test]
    fn text_range_each_style() {
        let t = table("nom,note\nbob,\"a|b\"\nann,\"x\ny\"\n", b',');
        assert_eq!(t.text_range(1..3, 0..2, Style::Tsv), "bob\ta|b\nann\t\"x\ny\"");
        assert_eq!(t.text_range(0..1, 0..2, Style::Tsv), "nom\tnote");
        assert_eq!(t.text_range(0..2, 0..2, Style::Csv(b';')), "nom;note\nbob;a|b");
        assert_eq!(t.text_range(1..2, 1..2, Style::Csv(b'|')), "\"a|b\"");
        assert_eq!(
            t.text_range(0..3, 0..2, Style::Markdown),
            "| nom | note |\n| --- | --- |\n| bob | a\\|b |\n| ann | x y |"
        );
        assert_eq!(
            t.text_range(0..3, 0..2, Style::Json),
            "[\n  {\"nom\": \"bob\", \"note\": \"a|b\"},\n  {\"nom\": \"ann\", \"note\": \"x\\ny\"}\n]"
        );
        assert_eq!(t.text_range(0..1, 0..2, Style::Json), "[]");
        assert_eq!(t.text_range(0..0, 0..2, Style::Tsv), "");
        assert_eq!(t.text_range(0..9, 1..9, Style::Tsv), "note\na|b\n\"x\ny\"");
    }

    #[test]
    fn json_escapes() {
        let t = table("k\n\"q\"\"\\\"\n", b',');
        assert_eq!(t.text_range(0..2, 0..1, Style::Json), "[\n  {\"k\": \"q\\\"\\\\\"}\n]");
        let mut out = String::new();
        json_string(&mut out, "\u{1}\t");
        assert_eq!(out, "\"\\u0001\\t\"");
    }

    #[test]
    fn styles_and_extensions() {
        for s in [Style::Tsv, Style::Csv(b','), Style::Markdown, Style::Json] {
            assert_eq!(Style::from_extension(s.extension()), Some(s));
        }
        assert_eq!(Style::from_extension("CSV"), Some(Style::Csv(b',')));
        assert_eq!(Style::from_extension("txt"), None);
    }

    #[test]
    fn parse_grid_variants() {
        assert_eq!(parse_grid("a\tb\r\nc\td\r\n"), vec![vec!["a", "b"], vec!["c", "d"]]);
        assert_eq!(parse_grid("a,b\nc,\"d,e\"\n"), vec![vec!["a", "b"], vec!["c", "d,e"]]);
        assert_eq!(parse_grid("a;b;c\n1;2;3"), vec![vec!["a", "b", "c"], vec!["1", "2", "3"]]);
        assert_eq!(parse_grid("bonjour"), vec![vec!["bonjour"]]);
        assert_eq!(parse_grid("une phrase, avec virgule"), vec![vec!["une phrase, avec virgule"]]);
        assert_eq!(parse_grid("deux mots\n"), vec![vec!["deux mots"]]);
        assert!(parse_grid("").is_empty());
    }

    #[test]
    fn delimiter_labels_cover_all() {
        for d in DELIMITERS {
            assert_ne!(delimiter_label(d), tr("Other", "Autre"));
        }
    }

    /// Mémoire résidente du processus, en Mo (Linux).
    fn rss_mb() -> f64 {
        std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| s.split_whitespace().nth(1)?.parse::<f64>().ok())
            .map_or(0.0, |pages| pages * 4096.0 / 1_048_576.0)
    }

    #[test]
    #[ignore]
    fn scale() {
        let t0 = Instant::now();
        let mut s = String::with_capacity(48_000_000);
        for i in 0..1_000_000u32 {
            writeln!(s, "{i},nom{},{}.5,\"a, b\",x{},ligne {},{},fin", i % 977, i % 31, i % 13, i % 7, i * 3)
                .unwrap();
        }
        let mb = s.len() as f64 / 1_048_576.0;
        println!("génération : {:?}, texte {mb:.1} Mo, RSS {:.0} Mo", t0.elapsed(), rss_mb());

        let rss0 = rss_mb();
        let t0 = Instant::now();
        let mut t = Table::parse(s, b',');
        println!(
            "parse : {:?} ({} lignes x {} colonnes), RSS {:.0} Mo (+{:.0} Mo)",
            t0.elapsed(),
            t.rows(),
            t.cols(),
            rss_mb(),
            rss_mb() - rss0
        );
        assert_eq!((t.rows(), t.cols()), (1_000_000, 8));

        let t0 = Instant::now();
        let mut n = 0;
        for r in (0..t.rows()).step_by(1000) {
            n += t.cell(r, 7).len() + t.cell(r, 3).len();
        }
        println!("1000 lectures de cellules : {:?} ({n})", t0.elapsed());

        let t0 = Instant::now();
        let snap = t.snapshot();
        println!("snapshot : {:?}", t0.elapsed());
        t.set_cell(500_000, 1, "x,y".into());
        let t0 = Instant::now();
        let mut sink = io::sink();
        snap.write_to(&mut sink, Encoding::Utf8).unwrap();
        t.write_to(&mut sink, Encoding::Utf16Le).unwrap();
        println!("écriture UTF-8 + UTF-16 (vers /dev/null) : {:?}", t0.elapsed());
    }

    #[test]
    fn reads_a_range_of_cells_in_one_pass() {
        let t = Table::parse("a,\"b,1\",c,d\ne\n".to_string(), b',');
        let cells = |row, from, to| t.cells(row, from..to).iter().map(|c| c.to_string()).collect::<Vec<_>>();
        assert_eq!(cells(0, 1, 3), ["b,1", "c"]);
        assert_eq!(cells(0, 2, 6), ["c", "d", "", ""]);
        assert_eq!(cells(1, 0, 3), ["e", "", ""]);
        assert_eq!(cells(9, 0, 2), ["", ""]);
        for c in 0..5 {
            assert_eq!(cells(0, c, c + 1)[0], t.cell(0, c));
        }
    }
}
