use crate::token::{Token, TokenType, Span};
use std::sync::OnceLock;

const INSERTED_TOKEN_ERROR_CODE: &str = INSERTED_TOKEN_ERROR_CODE;


fn default_max_errors() -> usize {
    static MAX: OnceLock<usize> = OnceLock::new();
    *MAX.get_or_init(|| {
        std::env::var("PARSER_MAX_ERRORS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|&n| n >= 1 && n <= 10_000)
            .unwrap_or(50)
    })
}


use std::collections::{HashMap, HashSet};
use std::cell::RefCell;
use std::rc::Rc;
use std::fmt;

/* ===================== AST ===================== */

#[derive(Debug, Clone)]
pub enum Expr {
    Int(String),
    Float(String),
    Str(String),
    Char(String),
    Ident(String),
    Binary { op: String, lhs: Box<Expr>, rhs: Box<Expr> },
    Unary { op: String, expr: Box<Expr> }, // + - ! ~ & *
    CallExpr { callee: Box<Expr>, args: Vec<Expr> },
    Assign { op: String, lhs: Box<Expr>, rhs: Box<Expr> },
    Ternary { cond: Box<Expr>, then_e: Box<Expr>, else_e: Box<Expr> },
    PostInc(Box<Expr>),
    PostDec(Box<Expr>),
    PreInc(Box<Expr>),
    PreDec(Box<Expr>),
    Index { base: Box<Expr>, index: Box<Expr> },
    Member { base: Box<Expr>, field: String },
    PtrMember { base: Box<Expr>, field: String },
    Comma(Vec<Expr>),

    // Cast / sizeof / alignof
    Cast { ty: CType, expr: Box<Expr> },
    SizeofExpr(Box<Expr>),
    SizeofType(CType),
    AlignofExpr(Box<Expr>),
    AlignofType(CType),
}

// 初始化器：= expr | = { init, ... }（可嵌套、允许拖尾逗号）
#[derive(Debug, Clone)]
pub enum Init {
    Expr(Expr),
    List(Vec<Init>),
}

// 极简类型：基础类型串 + 若干层 *
#[derive(Debug, Clone)]
pub struct CType {
    pub base: String,
    pub ptr: usize,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub ty: String,
    pub ptr: usize,
    pub name: String,
    pub array_dims: Vec<Option<String>>,
}

#[derive(Debug, Clone)]
pub struct Case {
    pub label: Option<Expr>, // None = default
    pub span: Span,          // 'case' 或 'default' 的位置
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    VarDecl {
        ty: String,
        ptr: usize,
        name: String,
        array_dims: Vec<Option<String>>,
        init: Option<Init>,
    },
    Return { value: Option<Expr>, span: Span },
    If {
        cond: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    While { cond: Expr, body: Box<Stmt> },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        step: Option<Expr>,
        body: Box<Stmt>,
    },
    Switch { expr: Expr, cases: Vec<Case> },
    DoWhile { body: Box<Stmt>, cond: Expr },

    Break(Span),
    Continue(Span),
    Goto { name: String, span: Span },
    Label { name: String, span: Span, stmt: Box<Stmt> },

    ExprStmt(Expr),
    Block(Vec<Stmt>),
    Empty,
}

/* ===== struct/union/enum 的 AST 节点 ===== */

#[derive(Debug, Clone)]
pub struct StructField {
    pub ty: String,
    pub ptr: usize,
    pub name: String,
    pub array_dims: Vec<Option<String>>,
    pub bit_width: Option<Expr>, // 位域宽度，如 `int flags:3;`
}

#[derive(Debug, Clone)]
pub enum StructKind {
    Struct,
    Union,
}

#[derive(Debug, Clone)]
pub struct EnumConst {
    pub name: String,
    pub value: Option<Expr>, // 允许 RED = 10 这样的表达式
}

#[derive(Debug, Clone)]
pub enum Item {
    Function {
        ret: String,
        ret_ptr: usize,
        name: String,
        name_span: Span,
        params: Vec<Param>,
        body: Stmt,
    },
    Global(Stmt),
    StructDef {
        kind: StructKind,
        name: String,
        fields: Vec<StructField>,
    },
    EnumDef {
        name: String,
        consts: Vec<EnumConst>,
    },
}

/* ===================== 错误结构（带 caret） ===================== */

#[derive(Debug, Clone)]
pub struct ParseError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,

    /// The source line at `span.line` (without trailing newline).
    pub line_text: String,
    /// Optional context line before `span.line`.
    pub prev_line: Option<(usize, String)>,
    /// Optional context line after `span.line`.
    pub next_line: Option<(usize, String)>,
    pub help: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseOutcome {
    Clean,
    RecoveredOnly,
    Failed,
}

impl fmt::Display for ParseOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseOutcome::Clean => write!(f, "clean"),
            ParseOutcome::RecoveredOnly => write!(f, "recovered"),
            ParseOutcome::Failed => write!(f, "failed"),
        }
    }
}


#[derive(Debug)]
pub struct ParseReport {
    pub items: Vec<Item>,
    pub outcome: ParseOutcome,
    pub errors: Vec<ParseError>,
}

impl ParseReport {
    pub fn is_clean(&self) -> bool {
        self.outcome == ParseOutcome::Clean
    }

    pub fn recovered_only(&self) -> bool {
        self.outcome == ParseOutcome::RecoveredOnly
    }

    pub fn failed(&self) -> bool {
        self.outcome == ParseOutcome::Failed
    }

    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    pub fn inserted_error_count(&self) -> usize {
        self.errors.iter().filter(|e| e.is_inserted()).count()
    }

    pub fn real_error_count(&self) -> usize {
        self.errors.len().saturating_sub(self.inserted_error_count())
    }

    pub fn error_quality_ratio(&self) -> f64 {
        if self.errors.is_empty() {
            return 0.0;
        }
        self.real_error_count() as f64 / self.errors.len() as f64
    }

    pub fn summary(&self) -> String {
        if self.errors.is_empty() {
            return "no parser errors".to_string();
        }

        let total = self.errors.len();
        let inserted = self.inserted_error_count();
        let real = total.saturating_sub(inserted);
        let ratio = self.error_quality_ratio();

        let first = self
            .errors
            .iter()
            .min_by_key(|e| e.sort_key())
            .map(|e| e.compact())
            .unwrap_or_else(|| "<unknown>".to_string());

        match self.outcome {
            ParseOutcome::Failed => {
                format!("{total} errors (real={real}, inserted={inserted}, ratio={ratio:.2}) (failed): {first}")
            }
            ParseOutcome::RecoveredOnly => {
                format!("{total} errors (real={real}, inserted={inserted}, ratio={ratio:.2}) (recovered): {first}")
            }
            ParseOutcome::Clean => "no parser errors".to_string(),
        }
    }
}

impl fmt::Display for ParseReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.outcome, self.summary())
    }
}




impl ParseError {
    pub fn render(&self) -> String {
        const TAB_WIDTH: usize = 4;
        const MAX_WIDTH: usize = 140;


        fn expand_tabs(s: &str) -> String {
            let mut out = String::with_capacity(s.len());
            let mut col = 0usize;
            for ch in s.chars() {
                match ch {
                    '\t' => {
                        let next = ((col / TAB_WIDTH) + 1) * TAB_WIDTH;
                        let spaces = next.saturating_sub(col).max(1);
                        out.extend(std::iter::repeat(' ').take(spaces));
                        col = next;
                    _ => {
                        out.push(ch);
                        col += 1;
                    }
                }
            }
            out
        }

        fn visual_col(prefix: &str) -> usize {
            let mut col = 0usize;
            for ch in prefix.chars() {
                if ch == '\t' {
                    let next = ((col / TAB_WIDTH) + 1) * TAB_WIDTH;
                    col = next;
                } else {
                    col += 1;
                }
            }
            col
        }

        // Prepare the main line (tabs expanded, CR/LF stripped).
        let raw_line = self.line_text.trim_end_matches(&['\r', '\n'][..]);
        let expanded = expand_tabs(raw_line);

        // span.col is 1-based; compute visual caret column (0-based).
        let prefix = raw_line
            .chars()
            .take(self.span.col.saturating_sub(1))
            .collect::<String>();
        let vcol = visual_col(&prefix);

        // Window the line if too long, keeping the caret in view.
        let mut start = 0usize;
        let expanded_chars: Vec<char> = expanded.chars().collect();
        if expanded_chars.len() > MAX_WIDTH {
            start = vcol.saturating_sub(MAX_WIDTH / 2);
        }
        if start > expanded_chars.len() {
            start = expanded_chars.len();
        }
        let mut end = (start + MAX_WIDTH).min(expanded_chars.len());
        if end == expanded_chars.len() && end.saturating_sub(MAX_WIDTH) < start {
            start = end.saturating_sub(MAX_WIDTH);
        }

        let mut shown_line: String = expanded_chars[start..end].iter().collect();
        let left_ellipsis = start > 0;
        let right_ellipsis = end < expanded_chars.len();
        if left_ellipsis {
            shown_line = format!("…{}", shown_line);
        }
        if right_ellipsis {
            shown_line.push('…');
        }

        let caret_pos = vcol.saturating_sub(start) + if left_ellipsis { 1 } else { 0 };
        let mut caret_len = self.span.len.max(1);
        let max_caret = MAX_WIDTH.saturating_sub(caret_pos).max(1);
        if caret_len > max_caret {
            caret_len = max_caret;
        }

        let mut caret = String::new();
        caret.push_str(&" ".repeat(caret_pos));
        caret.push_str(&"^".repeat(caret_len));

        // Dynamic gutter width based on the largest line number displayed.
        let mut max_line_no = self.span.line;
        if let Some((ln, _)) = &self.prev_line {
            max_line_no = max_line_no.max(*ln);
        }
        if let Some((ln, _)) = &self.next_line {
            max_line_no = max_line_no.max(*ln);
        }
        let w = std::cmp::max(4usize, max_line_no.to_string().len());
        let mk_prefix = |ln: usize| format!("{:>width$} | ", ln, width = w);
        let gutter = format!("{} | ", " ".repeat(w));

        let mut out = String::new();
        out.push_str(&format!(
            "{}: {} at {}:{}\n",
            self.code, self.message, self.span.line, self.span.col
        ));

        if let Some((ln, txt)) = &self.prev_line {
            let t = expand_tabs(txt.trim_end_matches(&['\r', '\n'][..]));
            out.push_str(&format!("{}{}\n", mk_prefix(*ln), t));
        }

        out.push_str(&format!("{}{}\n", mk_prefix(self.span.line), shown_line));
        out.push_str(&format!("{}{}\n", gutter, caret));

        if let Some((ln, txt)) = &self.next_line {
            let t = expand_tabs(txt.trim_end_matches(&['\r', '\n'][..]));
            out.push_str(&format!("{}{}\n", mk_prefix(*ln), t));
        }

        if let Some(h) = &self.help {
            out.push_str(&format!("help: {}\n", h));
        }
        out
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Keep Display concise; use render() when you want full, multi-line diagnostics.
        write!(f, "{}", self.compact())
    }
}

impl std::error::Error for ParseError {}


impl ParseError {
    /// Compact single-line representation: CODE line:col message
    pub fn compact(&self) -> String {
        format!("{} {}:{} {}", self.code, self.span.line, self.span.col, self.message)
    }

    /// Stable key for sorting/deduping errors by source position.
    pub fn sort_key(&self) -> (usize, usize, usize, &'static str) {
        (self.span.line, self.span.col, self.span.idx, self.code)
    }

    /// Whether this error came from an inserted-token recovery (e.g., missing ')' or ';').
    pub fn is_inserted(&self) -> bool {
        self.code == INSERTED_TOKEN_ERROR_CODE
    }

}




/* ===================== Parser ===================== */

pub struct Parser {
    tokens: Vec<Token>,
    i: usize,
    source: String,
    line_starts: Vec<usize>,
    max_errors_limit: usize,
    pub errors: Vec<ParseError>,

    seen_errors: HashSet<(usize, usize, usize, &'static str)>,
    sorted_errors_cache: RefCell<Option<Rc<Vec<usize>>>>,

    aborted: bool,

    // typedef 符号表（只存名字，不展开真实类型）
    typedefs: HashSet<String>,
}


fn build_line_starts(source: &str) -> Vec<usize> {
    // Byte offsets of each line start; line 1 starts at 0.
    let mut starts = Vec::with_capacity(128);
    starts.push(0);
    for (i, b) in source.as_bytes().iter().enumerate() {
        if *b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InfixKind {
    Binary,
    Assign,
    Ternary,
    Comma,
}

impl Parser {

    /// Override the maximum number of parser errors before aborting further parsing.
    /// Useful for test harnesses or batch runs.
    pub fn set_max_errors(&mut self, limit: usize) {
        self.max_errors_limit = limit.max(1);
    }

    /// Construct a parser with a custom maximum error limit (default is read from PARSER_MAX_ERRORS or 50).
    pub fn with_max_errors(tokens: Vec<Token>, source: String, limit: usize) -> Self {
        let mut p = Self::new(tokens, source);
        p.set_max_errors(limit);
        p
    }

    /// Get the current maximum error limit (after which parsing aborts).
    pub fn max_errors_limit(&self) -> usize {
        self.max_errors_limit
    }

    /// Whether parsing was aborted due to too many errors.
    pub fn aborted(&self) -> bool {
        self.aborted
    }

    /// Whether any parser errors were recorded.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Take ownership of accumulated errors (leaves the parser with an empty error list).
    pub fn take_errors(&mut self) -> Vec<ParseError> {
        *self.sorted_errors_cache.borrow_mut() = None;
        std::mem::take(&mut self.errors)
    }

    /// Clear accumulated errors and reset related error-tracking state.
    /// This does NOT reset the token stream position.
    pub fn clear_errors(&mut self) {
        self.errors.clear();
        self.seen_errors.clear();
        self.aborted = false;
        *self.sorted_errors_cache.borrow_mut() = None;
    }


    /// Convenience helper: parse all items and return (items, errors, aborted).
    /// Useful for callers that prefer a single return value rather than inspecting parser state.
    pub fn parse_result(mut self) -> (Vec<Item>, Vec<ParseError>, bool) {
        let items = self.parse_items();
        let aborted = self.aborted;
        let errors = self.errors;
        (items, errors, aborted)
    }

    /// Parse and return Ok(items) if no errors were produced; otherwise returns Err(errors).
    /// Note: if parsing aborted due to too many errors, this will also return Err(errors).
    pub fn parse_or_errors(self) -> Result<Vec<Item>, Vec<ParseError>> {
        let (items, errors, _aborted) = self.parse_result();
        if errors.is_empty() {
            Ok(items)
        } else {
            Err(errors)
        }

    /// Parse and return items together with the final high-level outcome and collected errors.
    pub fn parse_with_outcome(mut self) -> (Vec<Item>, ParseOutcome, Vec<ParseError>) {
        let items = self.parse_items();
        let outcome = self.outcome();
        let errors = self.errors;
        (items, outcome, errors)
    }

    /// Parse and return a structured report object.
    pub fn parse_report(mut self) -> ParseReport {
        let items = self.parse_items();
        let outcome = self.outcome();
        let errors = self.errors;
        ParseReport { items, outcome, errors }
    }


    }




    /// Number of errors collected so far.
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Count how many errors are inserted-token recoveries.
    pub fn inserted_error_count(&self) -> usize {
        self.errors.iter().filter(|e| e.is_inserted()).count()
    }

    /// Count non-inserted errors (i.e., "real" parse errors).
    pub fn real_error_count(&self) -> usize {
        self.errors.len().saturating_sub(self.inserted_error_count())
    }

    /// Ratio of real errors to total errors (0.0..=1.0). Higher is "worse" input; lower means more recovered via insertions.
    pub fn error_quality_ratio(&self) -> f64 {
        if self.errors.is_empty() {
            return 0.0;
        }
        self.real_error_count() as f64 / self.errors.len() as f64
    }




    /// Whether parsing aborted due to too many errors.
    pub fn has_aborted(&self) -> bool {
        self.aborted
    }

    /// True if no errors were produced and parsing did not abort.
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty() && !self.aborted
    }

    /// True if the parse should be treated as a failure by callers.
    /// Inserted-token recoveries alone do not count as a hard failure.
    pub fn should_fail(&self) -> bool {
        self.has_aborted() || self.real_error_count() > 0
    }

    /// True if parsing only needed inserted-token recovery and produced no "real" errors.
    pub fn recovered_only(&self) -> bool {
        !self.has_aborted() && self.real_error_count() == 0 && self.inserted_error_count() > 0
    }

    /// High-level parser outcome for callers/tests.
    pub fn outcome(&self) -> ParseOutcome {
        if self.should_fail() {
            ParseOutcome::Failed
        } else if self.recovered_only() {
            ParseOutcome::RecoveredOnly
        } else {
            ParseOutcome::Clean
        }
    }



    /// A short human-readable summary for logs: error count, aborted flag, and the first error (if any).
        pub fn summary(&self) -> String {
        if self.errors.is_empty() {
            return "no parser errors".to_string();
        }

        let total = self.errors.len();
        let inserted = self.inserted_error_count();
        let real = total.saturating_sub(inserted);
        let ratio = self.error_quality_ratio();

        let first = self
            .first_error()
            .map(|e| e.compact())
            .unwrap_or_else(|| "<unknown>".to_string());

        if self.aborted {
            format!(
                "{total} errors (real={real}, inserted={inserted}, ratio={ratio:.2}) (aborted): {first}"
            )
        } else {
            format!("{total} errors (real={real}, inserted={inserted}, ratio={ratio:.2}): {first}")
        }
    }
        let first = self.first_error().map(|e| e.compact()).unwrap_or_else(|| "<unknown>".to_string());
        if self.aborted {
            format!("{} errors (aborted): {}", self.errors.len(), first)
        } else {
            format!("{} errors: {}", self.errors.len(), first)
        }
    }



    /// Count errors by error code (stable ordering).
    pub fn error_stats(&self) -> std::collections::BTreeMap<&'static str, usize> {
        let mut map: std::collections::BTreeMap<&'static str, usize> = std::collections::BTreeMap::new();
        for e in &self.errors {
            *map.entry(e.code).or_insert(0) += 1;
        }

    /// Count errors by code, split into (total, inserted).
    pub fn error_stats_detailed(&self) -> std::collections::BTreeMap<&'static str, (usize, usize)> {
        let mut map: std::collections::BTreeMap<&'static str, (usize, usize)> =
            std::collections::BTreeMap::new();
        for e in &self.errors {
            let entry = map.entry(e.code).or_insert((0, 0));
            entry.0 += 1;
            if e.is_inserted() {
                entry.1 += 1;
            }
        }
        map
    }


    fn sorted_error_indices(&self) -> Vec<usize> {
        let idxs = self.sorted_error_indices();

        idxs
    }

    fn append_errors_sorted_full(&self, out: &mut String) {
        self.append_errors_sorted_full(&mut out);

            }

    fn append_errors_sorted_compact(&self, out: &mut String) {
        self.append_errors_sorted_compact(&mut out);
    }

    fn append_errors_sorted_full_filtered(&self, out: &mut String, include_inserted: bool) {
        let idxs = self.sorted_error_indices();
        for &i in idxs.iter() {
            let e = &self.errors[i];
            if !include_inserted && e.is_inserted() {
                continue;
            }
            out.push_str(&e.render());
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
    }

    fn append_errors_sorted_compact_filtered(&self, out: &mut String, include_inserted: bool) {
        let idxs = self.sorted_error_indices();
        for &i in idxs.iter() {
            let e = &self.errors[i];
            if !include_inserted && e.is_inserted() {
                continue;
            }
            out.push_str(&e.compact());
            out.push('\n');
        }
    }




    /// Render all accumulated errors into a single string (useful for tests/logging).
    /// If `include_stats` is true, a short per-code summary is prepended.
    pub fn format_errors(&self, include_stats: bool) -> String {
        let mut out = String::new();

        if include_stats && !self.errors.is_empty() {
            out.push_str("== Parser error stats ==\n");
            for (code, n) in self.error_stats() {
                out.push_str(&format!("{code}: {n}\n"));
            }

    /// Render full multi-line errors, sorted by source position (line/col/idx/code).
    /// This keeps output stable for test diffs even if recovery changes emission order.
    pub fn format_errors_sorted(&self, include_stats: bool) -> String {
        let mut out = String::new();

        if include_stats && !self.errors.is_empty() {
            out.push_str("== Parser error stats ==\n");
            for (code, n) in self.error_stats() {
                out.push_str(&format!("{code}: {n}\n"));
            }

    /// Render only non-inserted ("real") errors in stable source order.
    pub fn format_real_errors_sorted(&self, include_stats: bool) -> String {
        let mut out = String::new();

        if include_stats && !self.errors.is_empty() {
            out.push_str("== Parser error stats (real only) ==\n");
            for (code, (total, inserted)) in self.error_stats_detailed() {
                let real = total.saturating_sub(inserted);
                if real == 0 {
                    continue;
                }
                out.push_str(&format!("{code}: {real}\n"));
            }
            out.push('\n');
        }

        self.append_errors_sorted_full_filtered(&mut out, false);
        out
    }

    /// Render only non-inserted ("real") errors in a compact, one-line-per-error format.
    pub fn format_real_errors_compact_sorted(&self) -> String {
        let mut out = String::new();
        self.append_errors_sorted_compact_filtered(&mut out, false);
        out
    }


    /// Like `format_errors_sorted`, but the stats header also reports inserted-token counts per code.
    pub fn format_errors_sorted_detailed_stats(&self) -> String {
        if self.errors.is_empty() {
            return String::new();
        }

        let mut out = String::new();
        out.push_str("== Parser error stats (total/inserted) ==\n");
        for (code, (total, inserted)) in self.error_stats_detailed() {
            out.push_str(&format!("{code}: {total}/{inserted}\n"));
        }
        out.push('\n');

        self.append_errors_sorted_full(&mut out);

                out
    }


    /// Get errors sorted by source position (line/col/idx/code).
    pub fn errors_sorted(&self) -> Vec<&ParseError> {
        let idxs = self.sorted_error_indices();
        let mut v = Vec::with_capacity(idxs.len());
        for &i in idxs.iter() {
            v.push(&self.errors[i]);
        }

    /// Call `f` for each error in stable source order without allocating a Vec of references.
    pub fn for_each_error_sorted<F: FnMut(&ParseError)>(&self, mut f: F) {
        let idxs = self.sorted_error_indices();
        for &i in idxs.iter() {
            f(&self.errors[i]);
        }
    }

        v
    }

    /// First error in source order, if any.
    pub fn first_error(&self) -> Option<&ParseError> {
        // O(n) without allocation/sort
        self.errors.iter().min_by_key(|e| e.sort_key())
    }

    /// Last error in source order, if any.
    pub fn last_error(&self) -> Option<&ParseError> {
        // O(n) without allocation/sort
        self.errors.iter().max_by_key(|e| e.sort_key())
    }


            out.push('\n');
        }

        self.append_errors_sorted_full(&mut out);

                out
    }


    /// Render errors in a compact, one-line-per-error format (easy to diff in tests).
    pub fn format_errors_compact(&self) -> String {
        let mut out = String::new();
        for e in &self.errors {
            out.push_str(&e.compact());
            out.push('\n');
        }

    /// Render errors in a compact format, but sorted by source position (line/col),
    /// to keep output stable even if recovery changes error emission order.
    pub fn format_errors_compact_sorted(&self) -> String {
        let idxs = self.sorted_error_indices();

        let mut out = String::new();
        for &i in idxs.iter() {
            out.push_str(&self.errors[i].compact());
            out.push('\n');
        }
        out
    }

        out
    }

            out.push('\n');
        }

        for e in &self.errors {
            out.push_str(&e.render());
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        out
    }

        map
    }




    pub fn new(tokens: Vec<Token>, source: String) -> Self {
        Self {
            tokens,
            i: 0,
            source,
            line_starts: build_line_starts(&source),
            max_errors_limit: default_max_errors(),
            errors: vec![],
            seen_errors: HashSet::new(),
            sorted_errors_cache: RefCell::new(None),
            aborted: false,
            typedefs: HashSet::new(),
        }
    }

    #[inline]
    fn at_end(&self) -> bool {
        self.i >= self.tokens.len()
    }
    #[inline]
    fn cur(&self) -> Option<&Token> {
        self.tokens.get(self.i)
    }
    #[inline]
    fn cur_is(&self, k: &TokenType) -> bool {
        self.cur().map(|t| t.kind() == k).unwrap_or(false)
    }
    #[inline]
    fn cur_is_kw(&self, kw: &str) -> bool {
        self.cur()
            .map(|t| matches!(t.kind(), TokenType::Keyword) && t.text() == kw)
            .unwrap_or(false)
    }
    #[inline]
    fn cur_is_punct(&self, ch: &str) -> bool {
        self.cur()
            .map(|t| matches!(t.kind(), TokenType::Punctuation) && t.text() == ch)
            .unwrap_or(false)
    }
    #[inline]
    fn cur_is_op(&self, op: &str) -> bool {
        self.cur()
            .map(|t| matches!(t.kind(), TokenType::Operator) && t.text() == op)
            .unwrap_or(false)
    }
    fn bump(&mut self) -> Option<Token> {
        if self.at_end() {
            None
        } else {
            let t = self.tokens[self.i].clone();
            self.i += 1;
            Some(t)
        }
    }

    fn save(&self) -> usize {
        self.i
    }
    fn restore(&mut self, mark: usize) {
        self.i = mark;
    }

    fn peek_prev_span(&self) -> Span {
        if self.i > 0 {
            self.tokens[self.i - 1].span()
        } else {
            Span {
                line: 1,
                col: 1,
                idx: 0,
                len: 0,
            }
        }
    }
    fn cur_span(&self) -> Span {
        if let Some(t) = self.cur() {
            t.span()
        } else {
            self.peek_prev_span()
        }
    }

    fn line_text_at(&self, span: Span) -> String {
        self.line_text_by_no(span.line)
    }

    fn line_text_by_no(&self, line_no: usize) -> String {
        if line_no == 0 {
            return String::new();
        }
        let line_idx = line_no.saturating_sub(1);
        if line_idx >= self.line_starts.len() {
            return String::new();
        }
        let start = self.line_starts[line_idx];
        let end = if line_idx + 1 < self.line_starts.len() {
            self.line_starts[line_idx + 1].saturating_sub(1) // exclude trailing '\n'
        } else {
            self.source.len()
        };
        self.source
            .get(start..end)
            .unwrap_or("")
            .trim_end_matches(&['\r', '\n'][..])
            .to_string()
    }

    
    fn push_error(&mut self, code: &'static str, message: String, span: Span, help: Option<String>) {
        if self.aborted {
            return;
        }

        // Deduplicate repeated errors at the same position (common during recovery).
        let key = (span.line, span.col, span.idx, code);
        if self.seen_errors.contains(&key) {
            return;
        }
        self.seen_errors.insert(key);

        // Reserve the last slot for a final "too many errors" message.
        if self.errors.len() >= self.max_errors_limit.saturating_sub(1) {
            self.aborted = true;

            let abort_span = span;
            let abort_line_text = self.line_text_at(abort_span);

            self.errors.push(ParseError {
                code: "E9999",
                message: format!(
                    "too many errors (limit {}), aborting parse",
                    self.max_errors_limit
                ),
                span: abort_span,
                line_text: abort_line_text,
                prev_line: if abort_span.line > 1 {
                    Some((abort_span.line - 1, self.line_text_by_no(abort_span.line - 1)))
                } else {
                    None
                },
                next_line: Some((abort_span.line + 1, self.line_text_by_no(abort_span.line + 1)))
                    .filter(|(_, s)| !s.is_empty()),
                help: Some(
                    "Set PARSER_MAX_ERRORS or call Parser::set_max_errors(...) to adjust the limit."
                        .into(),
                ),
            });
        *self.sorted_errors_cache.borrow_mut() = None;

            // Force parsing loops to stop cleanly.
            self.i = self.tokens.len();
            return;
        }

        let line_text = self.line_text_at(span);
        self.errors.push(ParseError {
            code,
            message,
            span,
            line_text,
            prev_line: if span.line > 1 {
                Some((span.line - 1, self.line_text_by_no(span.line - 1)))
            } else {
                None
            },
            next_line: Some((span.line + 1, self.line_text_by_no(span.line + 1)))
                .filter(|(_, s)| !s.is_empty()),
            help,
        });
    }

    fn err_push(&mut self, code: &'static str, message: String, span: Span) {
        self.push_error(code, message, span, None);
    }

    fn err_push_help(&mut self, code: &'static str, message: String, span: Span, help: Option<String>) {
        self.push_error(code, message, span, help);
    }



    fn err_expect(&mut self, expected: &str) {
        let mut span = self.cur_span();
        // Point the caret at the insertion position rather than highlighting the current token.
        span.len = 0;
        let got = if self.at_end() {
            "EOF".to_string()
        } else {
            let t = &self.tokens[self.i];
            // Keep messages stable (no raw newlines/tabs) and reasonably short.
            let mut s = t.text().to_string();
            s = s.replace('\n', "\\n")
                 .replace('\r', "\\r")
                 .replace('\t', "\\t");
            if s.len() > 40 {
                s.truncate(40);
                s.push('…');
            }

    fn err_expect_inserted(&mut self, expected: &str) {
        let span = self.cur_span();
        let got = if self.at_end() {
            "EOF".to_string()
        } else {
            let t = &self.tokens[self.i];
            let mut s = t.text().to_string();
            s = s.replace('\n', "\\n")
                 .replace('\r', "\\r")
                 .replace('\t', "\\t");
            if s.len() > 40 {
                s.truncate(40);
                s.push('…');
            }
            format!("{:?} '{}'", t.kind(), s)
        };
        // Note: we intentionally do NOT sync() here, treating the token as "inserted" for recovery.
        self.err_push_help(
            INSERTED_TOKEN_ERROR_CODE,
            format!("expected {} (inserted), found {}", expected, got),
            span,
            Some(format!("assuming missing {} here", expected)),
        );
    }

            format!("{:?} '{}'", t.kind(), s)
        };
        self.err_push("E1001", format!("expected {} (inserted), found {}", expected, got), span);
        self.sync();
    }
    fn err_custom_span(&mut self, code: &'static str, msg: String, span: Span) {
        self.err_push(code, msg, span);
        self.sync();
    }
    fn err_custom_here(&mut self, code: &'static str, msg: &str) {
        let span = self.cur_span();
        self.err_push(code, msg.to_string(), span);
        self.sync();
    }

    fn expect_punct(&mut self, ch: &str) {
        if !self.cur_is_punct(ch) {
            // Error production: treat common separators/closers as inserted to reduce cascade errors.
            let insertable = matches!(ch, ")" | "]" | "}" | ";" | "," | ":");
            if insertable && (self.is_expr_end() || self.cur_is_punct("}") || self.at_end()) {
                self.err_expect_inserted(&format!("'{}'", ch));
                return;
            }
            self.err_expect(&format!("'{}'", ch));
        } else {
            self.bump();
        }
    }    }
    fn expect_kw(&mut self, kw: &str) {
        if !self.cur_is_kw(kw) {
            self.err_expect(&format!("keyword '{}'", kw));
        } else {
            self.bump();
        }
    }
    fn expect_token_text(&mut self, text: &str) -> bool {
        if self.cur_is_punct(text) || self.cur_is_op(text) || self.cur_is_kw(text) {
            self.bump();
            true
        } else {
            self.err_expect(&format!("'{}'", text));
            false
        }
    }

    // 同步：在语句/表达式内部进行 panic-mode 恢复
    // 跳过 token，直到遇到一个“边界 token”（分隔符、闭合符、关键关键字等）
    fn sync(&mut self) {
        while !self.at_end() {
            // Consume separators so we don't get stuck reporting the same error.
            if self.cur_is_punct(";") || self.cur_is_punct(",") {
                self.bump();
                return;
            }

    /// Whether the current token can start a statement.
    fn is_stmt_start(&self) -> bool {
        if self.at_end() {
            return false;
        }
        // Keywords that can begin a statement
        if self.cur_is_kw("if")
            || self.cur_is_kw("for")
            || self.cur_is_kw("while")
            || self.cur_is_kw("do")
            || self.cur_is_kw("switch")
            || self.cur_is_kw("return")
            || self.cur_is_kw("break")
            || self.cur_is_kw("continue")
            || self.cur_is_kw("goto")
            || self.cur_is_kw("else")
        {
            return true;
        }
        // Block start
        if self.cur_is_punct("{") {
            return true;
        }
        // A declaration can also start a statement (e.g., "int x;")
        if self.peek_type_start() {
            return true;
        }
        // Fallback: identifier could start an expression-statement
        self.cur_is(&TokenType::Identifier)
    }

        /// Whether the current token can end an expression (common recovery boundary).
    fn is_expr_end(&self) -> bool {
        if self.at_end() {
            return true;
        }
        self.cur_is_punct(";")
            || self.cur_is_punct(")")
            || self.cur_is_punct("]")
            || self.cur_is_punct("}")
            || self.cur_is_punct(",")
            || self.cur_is_punct(":")
            || self.cur_is_kw("case")
            || self.cur_is_kw("default")
    }

/// Statement-level panic-mode recovery: skip tokens until a likely statement boundary.
    /// This reduces cascade errors compared to token-level sync().
    fn sync_stmt(&mut self) {
        // Ensure we always make progress
        if !self.at_end() {
            self.i += 1;
        }

        while !self.at_end() {
            // Hard statement boundaries
            if self.cur_is_punct(";") || self.cur_is_punct("}") {
                return;
            }

            // Switch label boundaries
            if self.cur_is_punct(":") || self.cur_is_kw("case") || self.cur_is_kw("default") || self.cur_is_kw("else") {
                return;
            }

            // Next likely statement start
            if self.is_stmt_start() {
                return;
            }

            self.i += 1;
        }
    }

        while !self.at_end() {
            if self.cur_is_punct(";") || self.cur_is_punct("}") {
                return;
            }
            if self.is_stmt_start() {
                return;
            }
            self.i += 1;
        }
    }


            // Do not consume closers / label separators: caller should decide how to unwind.
            if self.cur_is_punct("}")
                || self.cur_is_punct(")")
                || self.cur_is_punct("]")
                || self.cur_is_punct(":")
            {
                return;
            }

            // Switch labels / else: let the higher-level parser see them.
            if self.cur_is_kw("case") || self.cur_is_kw("default") || self.cur_is_kw("else") {
                return;
            }

            // Potential statement/decl starts: stop so outer loop can re-dispatch.
            if self.cur_is_kw("if")
                || self.cur_is_kw("for")
                || self.cur_is_kw("while")
                || self.cur_is_kw("do")
                || self.cur_is_kw("switch")
                || self.cur_is_kw("return")
                || self.cur_is_kw("break")
                || self.cur_is_kw("continue")
                || self.cur_is_kw("goto")
            || self.cur_is_kw("else")
                || self.cur_is_kw("typedef")
                || self.cur_is_kw("struct")
                || self.cur_is_kw("union")
                || self.cur_is_kw("enum")
            {
                return;
            }

            // Treat '{' as a reasonable boundary for statement recovery.
            if self.cur_is_punct("{") {
                return;
            }

            self.i += 1;
        }
    }

    // switch body 内部的同步：不要跳过后续 case/default
    fn sync_in_switch(&mut self) {
        while !self.at_end() {
            if self.cur_is_kw("case") || self.cur_is_kw("default") || self.cur_is_punct("}") {
                return;
            }
            if self.cur_is_punct(";") {
                self.bump();
                return;
            }
            self.bump();
        }
    }

    /// 顶层同步：用于 parse_items() 的 panic-mode 恢复。
    /// 目标：跳到下一个可能的“顶层起始点”，避免卡死在坏 token 上。
    fn sync_top_level(&mut self) {
        // Always ensure we make progress.
        if !self.at_end() {
            self.i += 1;
        }

        while !self.at_end() {
            self.skip_trivia();
            if self.at_end() {
                return;
            }

            // Strong boundaries
            if self.cur_is_punct(";") {
                self.bump(); // consume to avoid repeating the same error on next iteration
                return;
            }
            if self.cur_is_punct("}") {
                // Stray close at top level: consume so we can make progress.
                self.bump();
                return;
            }

            // Declaration/definition keywords
            if self.cur_is_kw("typedef")
                || self.cur_is_kw("struct")
                || self.cur_is_kw("union")
                || self.cur_is_kw("enum")
            {
                return;
            }

            // Type start (including typedef names)
            if self.peek_type_start() {
                return;
            }

            // Global statement starts (if allowed)
            if self.cur_is_kw("if")
                || self.cur_is_kw("for")
                || self.cur_is_kw("while")
                || self.cur_is_kw("do")
                || self.cur_is_kw("switch")
                || self.cur_is_kw("return")
                || self.cur_is_kw("break")
                || self.cur_is_kw("continue")
                || self.cur_is_kw("goto")
            || self.cur_is_kw("else")
            {
                return;
            }

            self.i += 1;
        }
    }

    fn skip_trivia(&mut self) {
        while let Some(t) = self.cur() {
            if matches!(
                t.kind(),
                TokenType::Whitespace | TokenType::Comment | TokenType::Preprocessor
            ) {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    /* ===================== 类型关键字 / typedef 辅助 ===================== */

    fn is_builtin_type_kw_token(t: &Token) -> bool {
        if !matches!(t.kind(), TokenType::Keyword) {
            return false;
        }
        matches!(
            t.text(),
            "void"
                | "char"
                | "short"
                | "int"
                | "long"
                | "signed"
                | "unsigned"
                | "float"
                | "double"
                | "const"
                | "volatile"
        )
    }

    fn is_storage_or_func_spec_kw(t: &Token) -> bool {
        if !matches!(t.kind(), TokenType::Keyword) {
            return false;
        }
        matches!(
            t.text(),
            "static" | "extern" | "auto" | "register" | "inline" | "_Thread_local"
        )
    }

    fn is_ptr_qualifier_kw(t: &Token) -> bool {
        if !matches!(t.kind(), TokenType::Keyword) {
            return false;
        }
        matches!(t.text(), "const" | "volatile" | "restrict")
    }

    fn is_tag_type_name(name: &str) -> bool {
        name.starts_with("struct ") || name.starts_with("union ") || name.starts_with("enum ")
    }

    fn peek_type_start(&self) -> bool {
        let mut j = self.i;
        while j < self.tokens.len() {
            let t = &self.tokens[j];

            if matches!(
                t.kind(),
                TokenType::Whitespace | TokenType::Comment | TokenType::Preprocessor
            ) {
                j += 1;
                continue;
            }

            if Self::is_storage_or_func_spec_kw(t) {
                j += 1;
                continue;
            }

            if Self::is_builtin_type_kw_token(t) {
                return true;
            }
            if matches!(t.kind(), TokenType::Identifier) && self.typedefs.contains(t.text()) {
                return true;
            }
            if matches!(t.text(), "struct" | "union" | "enum") {
                if let Some(nxt) = self.tokens.get(j + 1) {
                    if matches!(nxt.kind(), TokenType::Identifier) || matches!(nxt.text(), "{") {
                        return true;
                    }
                }
            }
            break;
        }
        false
    }

    fn parse_builtin_type_keyword_seq(&mut self) -> Option<Vec<String>> {
        if !self
            .cur()
            .map(Self::is_builtin_type_kw_token)
            .unwrap_or(false)
        {
            return None;
        }
        let mut specs = Vec::new();
        while let Some(t) = self.cur() {
            if Self::is_builtin_type_kw_token(t) {
                specs.push(t.text().to_string());
                self.bump();
            } else {
                break;
            }
        }
        Some(specs)
    }

    fn specs_to_string(specs: &[String]) -> String {
        specs.join(" ")
    }

    fn skip_brace_block_in_type(&mut self) {
        if !self.cur_is_punct("{") {
            return;
        }
        let mut depth = 0usize;
        while !self.at_end() {
            if self.cur_is_punct("{") {
                depth += 1;
            } else if self.cur_is_punct("}") {
                depth = depth.saturating_sub(1);
            }
            let _ = self.bump();
            if depth == 0 {
                break;
            }
        }
        if depth > 0 {
            self.err_custom_here("E3008", "unclosed brace in inline struct/union/enum definition");
        }
    }

    fn parse_decl_base_type(&mut self) -> Option<String> {
        loop {
            if let Some(t) = self.cur() {
                if Self::is_storage_or_func_spec_kw(t) {
                    self.bump();
                    continue;
                }
            }
            break;
        }

        if let Some(specs) = self.parse_builtin_type_keyword_seq() {
            return Some(Self::specs_to_string(&specs));
        }
        if let Some(t) = self.cur() {
            if matches!(t.kind(), TokenType::Identifier) && self.typedefs.contains(t.text()) {
                let name = t.text().to_string();
                self.bump();
                return Some(name);
            }
        }
        if self.cur_is_kw("struct") || self.cur_is_kw("union") || self.cur_is_kw("enum") {
            let kw_tok = self.bump().unwrap();
            let kw = kw_tok.text().to_string();

            if self.cur_is_punct("{") {
                self.skip_brace_block_in_type();
                let base = format!("{} <anon>", kw);
                return Some(base);
            }

            let tag = if self.cur_is(&TokenType::Identifier) {
                self.bump().unwrap().text().to_string()
            } else {
                self.err_custom_here("E5201", "expected tag name after 'struct'/'union'/'enum'");
                "_anon".into()
            };
            return Some(format!("{} {}", kw, tag));
        }
        None
    }

    fn parse_type_name_full(&mut self) -> Option<CType> {
        let mark = self.save();
        if let Some(specs) = self.parse_builtin_type_keyword_seq() {
            let base = Self::specs_to_string(&specs);
            let ptr = self.parse_pointer_stars();
            return Some(CType { base, ptr });
        }
        self.restore(mark);

        if let Some(t) = self.cur() {
            if matches!(t.kind(), TokenType::Identifier) && self.typedefs.contains(t.text()) {
                let base = t.text().to_string();
                self.bump();
                let ptr = self.parse_pointer_stars();
                return Some(CType { base, ptr });
            }
        }

        if self.cur_is_kw("struct") || self.cur_is_kw("union") || self.cur_is_kw("enum") {
            let kw_tok = self.bump().unwrap();
            let kw = kw_tok.text().to_string();

            if self.cur_is_punct("{") {
                self.skip_brace_block_in_type();
                let base = format!("{} <anon>", kw);
                let ptr = self.parse_pointer_stars();
                return Some(CType { base, ptr });
            }

            let tag = if self.cur_is(&TokenType::Identifier) {
                self.bump().unwrap().text().to_string()
            } else {
                self.err_custom_here("E5201", "expected tag name after 'struct'/'union'/'enum'");
                "_anon".into()
            };
            let base = format!("{} {}", kw, tag);
            let ptr = self.parse_pointer_stars();
            return Some(CType { base, ptr });
        }

        None
    }

    /* ===================== 入口 ===================== */

    pub fn parse_items(&mut self) -> Vec<Item> {
        let mut items = vec![];
        while !self.at_end() {
            self.skip_trivia();
            if self.at_end() {
                break;
            }

            let start_i = self.i;

            if self.cur_is_kw("typedef") {
                self.parse_typedef_decl();
                continue;
            }

            if self.cur_is_kw("struct") || self.cur_is_kw("union") {
                let mark = self.save();
                let kw_tok = self.bump().unwrap();
                let kind = if kw_tok.text() == "struct" {
                    StructKind::Struct
                } else {
                    StructKind::Union
                };
                let name = if self.cur_is(&TokenType::Identifier) {
                    self.bump().unwrap().text().to_string()
                } else {
                    self.err_custom_here("E5201", "expected tag name after 'struct'/'union'");
                    "_anon".into()
                };
                if self.cur_is_punct("{") {
                    let fields = self.parse_struct_body_fields();
                    if !self.expect_token_text(";") {
                        self.err_custom_here("E2001", "missing ';' after struct/union definition");
                    }
                    items.push(Item::StructDef { kind, name, fields });
                    continue;
                } else {
                    self.restore(mark);
                }
            }

            if self.cur_is_kw("enum") {
                let mark = self.save();
                self.bump(); // 'enum'
                let name = if self.cur_is(&TokenType::Identifier) {
                    self.bump().unwrap().text().to_string()
                } else {
                    self.err_custom_here("E5202", "expected enum tag name");
                    "_anon".into()
                };
                if self.cur_is_punct("{") {
                    let item = self.parse_enum_def(name);
                    items.push(item);
                    continue;
                } else {
                    self.restore(mark);
                }
            }

            if self.peek_type_start() {
                let base_ty = match self.parse_decl_base_type() {
                    Some(t) => t,
                    None => {
                        self.err_custom_here("E2100", "invalid type specifier");
                        continue;
                    }
                };
                let ret_ptr = self.parse_pointer_stars();

                if self.cur_is(&TokenType::Identifier) {
                    let name_tok = self.bump().unwrap();
                    let name = name_tok.text().to_string();
                    let name_span = name_tok.span();

                    if self.cur_is_punct("(") {
                        let params = self.parse_params();
                        let body = if self.cur_is_punct("{") {
                            self.parse_block()
                        } else {
                            self.err_custom_here("E2003", "function must have a body");
                            Stmt::Empty
                        };
                        items.push(Item::Function {
                            ret: base_ty,
                            ret_ptr,
                            name,
                            name_span,
                            params,
                            body,
                        });
                    } else {
                        let first_dims = self.parse_array_dims_multi();
                        let first_init = if self.cur_is_op("=") {
                            self.bump();
                            Some(self.parse_initializer())
                        } else {
                            None
                        };
                        let mut decls: Vec<(usize, String, Span, Vec<Option<String>>, Option<Init>)> =
                            vec![(ret_ptr, name, name_span, first_dims, first_init)];

                        while self.cur_is_punct(",") {
                            self.bump();
                            let ptr = self.parse_pointer_stars();
                            if self.cur_is(&TokenType::Identifier) {
                                let nm_tok = self.bump().unwrap();
                                let nm = nm_tok.text().to_string();
                                let nm_span = nm_tok.span();
                                let dims = self.parse_array_dims_multi();
                                let ini = if self.cur_is_op("=") {
                                    self.bump();
                                    Some(self.parse_initializer())
                                } else {
                                    None
                                };
                                decls.push((ptr, nm, nm_span, dims, ini));
                            } else {
                                self.err_custom_here(
                                    "E2102",
                                    "expected identifier after ',' in declaration",
                                );
                                break;
                            }
                        }
                        if !self.expect_token_text(";") {
                            self.err_custom_here("E2001", "missing ';' after declaration");
                        }

                        for (_, nm, nm_span, dims, ini) in &decls {
                            if let Some(init) = ini {
                                self.validate_array_initializer(nm, *nm_span, dims, init);
                            }
                        }

                        let mut stmts = vec![];
                        for (ptr, nm, _sp, dims, ini) in decls {
                            stmts.push(Stmt::VarDecl {
                                ty: base_ty.clone(),
                                ptr,
                                name: nm,
                                array_dims: dims,
                                init: ini,
                            });
                        }
                        items.push(Item::Global(if stmts.len() == 1 {
                            stmts.pop().unwrap()
                        } else {
                            Stmt::Block(stmts)
                        }));
                    }
                } else if self.cur_is_punct(";") && Self::is_tag_type_name(&base_ty) {
                    self.bump();
                    continue;
                } else {
                    self.err_custom_here("E2101", "expected identifier after type");
                }
            } else {
                let s = self.parse_stmt();
                items.push(Item::Global(s));
            }

            // panic-mode：如果本轮没有消费任何 token，说明进入无法前进的错误状态，进行顶层同步恢复
            if self.i == start_i && !self.at_end() {
                let sp = self.cur_span();
                self.err_push(
                    "E9000",
                    "parser made no progress at top level; skipping tokens to recover".to_string(),
                    sp,
                );
                self.sync_top_level();
                // 吃掉明显的分隔符/孤立闭合符，避免下一轮再次卡住
                if self.cur_is_punct(";") || self.cur_is_punct(",") || self.cur_is_punct("}") {
                    self.bump();
                }
            }

        }

        // 解析完所有 item 后，做语义检查
        self.check_labels_and_gotos(&items);
        self.check_loops_and_breaks(&items);
        self.check_function_returns(&items);
        self.check_switch_cases(&items);
        self.check_function_redefinitions(&items);
        self.check_unreachable(&items); // unreachable（增强版）

        items
    }

    /* ===================== typedef 解析 ===================== */

    fn parse_typedef_decl(&mut self) {
        self.expect_kw("typedef");

        let base_ty = match self.parse_decl_base_type() {
            Some(t) => t,
            None => {
                self.err_custom_here("E5100", "expected type name after 'typedef'");
                return;
            }
        };
        let _base_ptr = self.parse_pointer_stars();

        let mut first = true;
        loop {
            if !self.cur_is(&TokenType::Identifier) {
                if first {
                    self.err_custom_here("E5101", "expected typedef name");
                }
                break;
            }
            let nm_tok = self.bump().unwrap();
            let nm = nm_tok.text().to_string();
            let _dims = self.parse_array_dims_multi();

            self.typedefs.insert(nm);
            first = false;

            if !self.cur_is_punct(",") {
                break;
            }
            self.bump();
        }

        if !self.expect_token_text(";") {
            self.err_custom_here("E2001", "missing ';' after typedef");
        }
        let _ = base_ty;
    }

    /* ===================== struct/union/enum 定义体解析 ===================== */

    fn parse_struct_body_fields(&mut self) -> Vec<StructField> {
        self.expect_punct("{");
        let mut fields = Vec::new();

        loop {
            self.skip_trivia();
            if self.at_end() {
                self.err_custom_here("E3007", "unclosed struct/union body, expected '}' before EOF");
                break;
            }
            if self.cur_is_punct("}") {
                self.bump();
                break;
            }

            if !self.peek_type_start() {
                self.err_custom_here("E5300", "expected field type in struct/union");
                self.sync();
                continue;
            }

            let base_ty = self
                .parse_decl_base_type()
                .unwrap_or_else(|| "int".to_string());

            loop {
                let ptr = self.parse_pointer_stars();

                if self.cur_is(&TokenType::Identifier) {
                    let name_tok = self.bump().unwrap();
                    let name = name_tok.text().to_string();
                    let dims = self.parse_array_dims_multi();

                    let bit_width = if self.cur_is_punct(":") || self.cur_is_op(":") {
                        self.bump();
                        Some(self.parse_expr())
                    } else {
                        None
                    };

                    if self.cur_is_op("=") {
                        self.err_custom_here("E5302", "field declaration cannot have an initializer");
                        self.bump();
                        let _ = self.parse_initializer();
                    }

                    fields.push(StructField {
                        ty: base_ty.clone(),
                        ptr,
                        name,
                        array_dims: dims,
                        bit_width,
                    });
                } else if self.cur_is_punct(";") || self.cur_is_punct(":") {
                    let name = "<anon>".to_string();
                    let dims = self.parse_array_dims_multi();

                    let bit_width = if self.cur_is_punct(":") || self.cur_is_op(":") {
                        self.bump();
                        Some(self.parse_expr())
                    } else {
                        None
                    };

                    if self.cur_is_op("=") {
                        self.err_custom_here("E5302", "field declaration cannot have an initializer");
                        self.bump();
                        let _ = self.parse_initializer();
                    }

                    fields.push(StructField {
                        ty: base_ty.clone(),
                        ptr,
                        name,
                        array_dims: dims,
                        bit_width,
                    });
                } else {
                    self.err_custom_here("E5301", "expected field name or ';' in struct/union");
                    break;
                }

                if self.cur_is_punct(",") {
                    self.bump();
                    continue;
                } else {
                    break;
                }
            }

            if !self.expect_token_text(";") {
                self.err_custom_here("E2001", "missing ';' after struct/union field");
            }
        }

        fields
    }

    fn parse_enum_def(&mut self, name: String) -> Item {
        self.expect_punct("{");
        let mut consts = Vec::new();
        let mut used_names: HashSet<String> = HashSet::new();

        loop {
            self.skip_trivia();
            let iter_start = self.i;
            if self.at_end() {
                self.err_custom_here("E3006", "unclosed enum body, expected '}' before EOF");
                break;
            }
            if self.cur_is_punct("}") {
                self.bump();
                break;
            }

            if !self.cur_is(&TokenType::Identifier) {
                self.err_custom_here("E5401", "expected enumerator name in enum");
                self.sync();
                if self.cur_is_punct(",") {
                    self.bump();
                }
                continue;
            }

            let name_tok = self.bump().unwrap();
            let cname = name_tok.text().to_string();
            let cspan = name_tok.span();

            if !used_names.insert(cname.clone()) {
                self.err_custom_span(
                    "E5402",
                    format!("duplicate enumerator name '{}'", cname),
                    cspan,
                );
            }

            let value = if self.cur_is_op("=") {
                self.bump();
                Some(self.parse_expr())
            } else {
                None
            };
            consts.push(EnumConst {
                name: cname,
                value,
            });

            // Progress guard: avoid infinite loop on malformed enum bodies.
            if self.i == iter_start {
                let sp = self.cur_span();
                self.err_push(
                    "E9005",
                    "no progress while parsing enum body".to_string(),
                    sp,
                );
                self.sync();
                if self.cur_is_punct(",") { self.bump(); }
                if self.cur_is_punct("}") { self.bump(); }
                continue;
            }

            if self.cur_is_punct(",") {
                self.bump();
                if self.cur_is_punct("}") {
                    continue;
                }
            }
        }

        if !self.expect_token_text(";") {
            self.err_custom_here("E2001", "missing ';' after enum definition");
        }

        Item::EnumDef { name, consts }
    }

    /* ===================== 指针与数组维度 ===================== */

    fn parse_pointer_stars(&mut self) -> usize {
        let mut n = 0usize;
        while self.cur_is_op("*") {
            self.bump();
            n += 1;
            loop {
                let is_qual = {
                    if let Some(t) = self.cur() {
                        Self::is_ptr_qualifier_kw(t)
                    } else {
                        false
                    }
                };
                if is_qual {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        n
    }

    fn parse_array_dims_multi(&mut self) -> Vec<Option<String>> {
        let mut dims = Vec::new();
        while self.cur_is_punct("[") {
            self.bump();
            let dim = if self.cur_is(&TokenType::IntConstant) {
                Some(self.bump().unwrap().text().to_string())
            } else {
                None
            };
            if !self.cur_is_punct("]") {
                self.err_custom_here("E3002", "unterminated array dimension, expected ']'");
            }
            if self.at_end() {
                        self.err_expect_inserted("']'");
                    } else {
                        self.expect_punct("]");
                    }
            dims.push(dim);
        }
        dims
    }

    /* ===================== 参数 ===================== */

    fn parse_params(&mut self) -> Vec<Param> {
        self.expect_punct("(");
        let mut v = vec![];
        if !self.cur_is_punct(")") {
            loop {
                let param_start = self.i;
                if !self.peek_type_start() {
                    self.err_custom_here("E2103", "expected type in parameter");
                    break;
                }
                let ty = self
                    .parse_decl_base_type()
                    .unwrap_or_else(|| "int".to_string());
                let ptr = self.parse_pointer_stars();
                let name = if self.cur_is(&TokenType::Identifier) {
                    self.bump().unwrap().text().to_string()
                } else {
                    "_".into()
                };
                let dims = self.parse_array_dims_multi();
                v.push(Param {
                    ty,
                    ptr,
                    name,
                    array_dims: dims,
                });

                if self.i == param_start {
                    let sp = self.cur_span();
                    self.err_push(
                        "E9004",
                        "no progress while parsing parameter".to_string(),
                        sp,
                    );
                    self.sync();
                    if self.cur_is_punct(",") { self.bump(); }
                    break;
                }

                if self.cur_is_punct(")") {
                    break;
                }
                if !self.expect_token_text(",") {
                    if !self.cur_is_punct(")") {
                        self.err_custom_here("E2005", "missing ',' between parameters");
                    }
                    break;
                }
            }
        }
        if !self.cur_is_punct(")") {
            self.err_custom_here("E3001", "unclosed parameter list, expected ')'");
        }
        self.expect_punct(")");
        v
    }

    /* ===================== 语句 ===================== */

    fn parse_stmt(&mut self) -> Stmt {
        self.skip_trivia();
        if self.at_end() {
            return Stmt::Empty;
        }

        if self.cur_is_kw("case") {
            let span = self.cur_span();
            self.err_push(
                "E2903",
                "'case' label not inside switch statement".to_string(),
                span,
            );
            self.bump();
            while !self.at_end()
                && !self.cur_is_punct(":")
                && !self.cur_is_punct(";")
                && !self.cur_is_punct("}")
            {
                self.bump();
            }
            if self.cur_is_punct(":") || self.cur_is_punct(";") {
                self.bump();
            }
            return Stmt::Empty;
        }
        if self.cur_is_kw("default") {
            let span = self.cur_span();
            self.err_push(
                "E2904",
                "'default' label not inside switch statement".to_string(),
                span,
            );
            self.bump();
            while !self.at_end()
                && !self.cur_is_punct(":")
                && !self.cur_is_punct(";")
                && !self.cur_is_punct("}")
            {
                self.bump();
            }
            if self.cur_is_punct(":") || self.cur_is_punct(";") {
                self.bump();
            }
            return Stmt::Empty;
        }

        if self.cur_is(&TokenType::Identifier) {
            if let Some(next) = self.tokens.get(self.i + 1) {
                if matches!(next.kind(), TokenType::Punctuation) && next.text() == ":" {
                    let name_tok = self.bump().unwrap();
                    let name = name_tok.text().to_string();
                    let span = name_tok.span();
                    self.expect_token_text(":");
                    let inner = self.parse_stmt();
                    return Stmt::Label {
                        name,
                        span,
                        stmt: Box::new(inner),
                    };
                }
            }
        }

        if self.cur_is_punct("{") {
            return self.parse_block();
        }
        if self.peek_type_start() {
            return self.parse_var_decl_stmt();
        }
        if self.cur_is_kw("return") {
            return self.parse_return();
        }
        if self.cur_is_kw("if") {
            return self.parse_if();
        }
        if self.cur_is_kw("while") {
            return self.parse_while();
        }
        if self.cur_is_kw("for") {
            return self.parse_for();
        }
        if self.cur_is_kw("switch") {
            return self.parse_switch();
        }
        if self.cur_is_kw("do") {
            return self.parse_do_while();
        }
        if self.cur_is_kw("goto") {
            return self.parse_goto();
        }

        if self.cur_is_kw("break") {
            let tok = self.bump().unwrap();
            let span = tok.span();
            if !self.expect_token_text(";") {
                self.err_custom_here("E2002", "missing ';' after 'break'");
            }
            return Stmt::Break(span);
        }
        if self.cur_is_kw("continue") {
            let tok = self.bump().unwrap();
            let span = tok.span();
            if !self.expect_token_text(";") {
                self.err_custom_here("E2002", "missing ';' after 'continue'");
            }
            return Stmt::Continue(span);
        }
        if self.cur_is_punct(";") {
            self.bump();
            return Stmt::Empty;
        }
        self.parse_expr_stmt()
    }

    fn parse_block(&mut self) -> Stmt {
        self.expect_punct("{");
        let mut v = vec![];
        loop {
            self.skip_trivia();
            if self.at_end() {
                self.err_custom_here("E3003", "unclosed block, expected '}' before EOF");
                break;
            }
            if self.cur_is_punct("}") {
                self.bump();
                break;
            }

            // Progress guard: if a statement parse fails to consume any token,
            // recover to avoid infinite loops.
            let start_i = self.i;
            v.push(self.parse_stmt());
            if self.i == start_i && !self.at_end() {
                let sp = self.cur_span();
                self.err_push(
                    "E9001",
                    "parser made no progress in block; skipping tokens to recover".to_string(),
                    sp,
                );
                self.sync();
                if self.cur_is_punct("}") {
                    self.bump();
                    break;
                }
            }
        }
        Stmt::Block(v)
    }

    fn parse_var_decl_stmt(&mut self) -> Stmt {
        let base_ty = match self.parse_decl_base_type() {
            Some(t) => t,
            None => {
                self.err_custom_here("E2100", "invalid type specifier");
                "int".to_string()
            }
        };

        let first_ptr = self.parse_pointer_stars();

        if self.cur_is_punct(";") && Self::is_tag_type_name(&base_ty) {
            self.bump();
            return Stmt::Empty;
        }

        let name_tok = if self.cur_is(&TokenType::Identifier) {
            self.bump().unwrap()
        } else {
            self.err_custom_here("E2104", "expected identifier in declaration");
            self.bump()
                .unwrap_or_else(|| self.tokens[self.i.saturating_sub(1)].clone())
        };
        let name = name_tok.text().to_string();
        let name_span = name_tok.span();

        let first_dims = self.parse_array_dims_multi();
        let init = if self.cur_is_op("=") {
            self.bump();
            Some(self.parse_initializer())
        } else {
            None
        };

        let mut decls: Vec<(usize, String, Span, Vec<Option<String>>, Option<Init>)> =
            vec![(first_ptr, name, name_span, first_dims, init)];

        while self.cur_is_punct(",") {
            self.bump();
            let ptr = self.parse_pointer_stars();
            if self.cur_is(&TokenType::Identifier) {
                let nm_tok = self.bump().unwrap();
                let nm = nm_tok.text().to_string();
                let nm_span = nm_tok.span();
                let dims = self.parse_array_dims_multi();
                let ini = if self.cur_is_op("=") {
                    self.bump();
                    Some(self.parse_initializer())
                } else {
                    None
                };
                decls.push((ptr, nm, nm_span, dims, ini));
            } else {
                self.err_custom_here("E2102", "expected identifier after ',' in declaration");
                break;
            }
        }

        if !self.expect_token_text(";") {
            self.err_custom_here("E2001", "missing ';' after declaration");
        }

        for (_, nm, nm_span, dims, ini) in &decls {
            if let Some(init) = ini {
                self.validate_array_initializer(nm, *nm_span, dims, init);
            }
        }

        let mut v = vec![];
        for (ptr, nm, _sp, dims, ini) in decls {
            v.push(Stmt::VarDecl {
                ty: base_ty.clone(),
                ptr,
                name: nm,
                array_dims: dims,
                init: ini,
            });
        }
        if v.len() == 1 {
            v.pop().unwrap()
        } else {
            Stmt::Block(v)
        }
    }

    fn parse_return(&mut self) -> Stmt {
        let tok = if self.cur_is_kw("return") {
            self.bump().unwrap()
        } else {
            self.err_custom_here("E2500", "expected 'return'");
            self.bump()
                .unwrap_or_else(|| self.tokens[self.i.saturating_sub(1)].clone())
        };
        let span = tok.span();

        if self.cur_is_punct(";") {
            self.bump();
            return Stmt::Return { value: None, span };
        }
        let e = self.parse_expr();
        if !self.expect_token_text(";") {
            self.err_custom_here("E2002", "missing ';' after return value");
        }
        Stmt::Return {
            value: Some(e),
            span,
        }
    }

    fn parse_if(&mut self) -> Stmt {
        self.expect_kw("if");
        if !self.expect_token_text("(") {
            self.err_custom_here("E3004", "missing '(' after 'if'");
        }
        let cond = self.parse_expr();
        if !self.expect_token_text(")") {
            self.err_custom_here("E3001", "unclosed condition, expected ')'");
        }
        let then_branch = self.parse_stmt();
        let else_branch = if self.cur_is_kw("else") {
            self.bump();
            Some(Box::new(self.parse_stmt()))
        } else {
            None
        };
        Stmt::If {
            cond,
            then_branch: Box::new(then_branch),
            else_branch,
        }
    }

    fn parse_while(&mut self) -> Stmt {
        self.expect_kw("while");
        if !self.expect_token_text("(") {
            self.err_custom_here("E3004", "missing '(' after 'while'");
        }
        let cond = self.parse_expr();
        if !self.expect_token_text(")") {
            self.err_custom_here("E3001", "unclosed condition, expected ')'");
        }
        let body = self.parse_stmt();
        Stmt::While {
            cond,
            body: Box::new(body),
        }
    }

    fn parse_do_while(&mut self) -> Stmt {
        self.expect_kw("do");
        let body = self.parse_stmt();

        if !self.cur_is_kw("while") {
            self.err_custom_here("E3009", "expected 'while' after 'do' body");
        } else {
            self.bump();
        }

        if !self.expect_token_text("(") {
            self.err_custom_here("E3010", "missing '(' after 'while' in do-while statement");
        }

        let cond = self.parse_expr();

        if !self.expect_token_text(")") {
            self.err_custom_here("E3011", "unclosed condition in do-while, expected ')'");
        }

        if !self.expect_token_text(";") {
            self.err_custom_here("E2002", "missing ';' after do-while");
        }

        Stmt::DoWhile {
            body: Box::new(body),
            cond,
        }
    }

    fn parse_goto(&mut self) -> Stmt {
        self.expect_kw("goto");
        let (name, span) = if self.cur_is(&TokenType::Identifier) {
            let tok = self.bump().unwrap();
            (tok.text().to_string(), tok.span())
        } else {
            let sp = self.cur_span();
            self.err_custom_here("E2601", "expected label name after 'goto'");
            ("_".into(), sp)
        };
        if !self.expect_token_text(";") {
            self.err_custom_here("E2002", "missing ';' after 'goto'");
        }
        Stmt::Goto { name, span }
    }

    fn parse_for(&mut self) -> Stmt {
        self.expect_kw("for");
        if !self.expect_token_text("(") {
            self.err_custom_here("E3004", "missing '(' after 'for'");
        }

        let init = if self.cur_is_punct(";") {
            self.bump();
            None
        } else if self.peek_type_start() {
            Some(Box::new(self.parse_var_decl_stmt()))
        } else {
            Some(Box::new(self.parse_expr_stmt()))
        };

        let cond = if self.cur_is_punct(";") {
            self.bump();
            None
        } else {
            let e = self.parse_expr();
            if !self.expect_token_text(";") {
                self.err_custom_here("E2006", "missing ';' in for-header after condition");
            }
            Some(e)
        };

        let step = if self.cur_is_punct(")") {
            None
        } else {
            Some(self.parse_expr())
        };

        if !self.expect_token_text(")") {
            self.err_custom_here("E3001", "unclosed for-header, expected ')'");
        }
        let body = self.parse_stmt();
        Stmt::For {
            init,
            cond,
            step,
            body: Box::new(body),
        }
    }

    fn parse_switch(&mut self) -> Stmt {
        self.expect_kw("switch");
        if !self.expect_token_text("(") {
            self.err_custom_here("E3004", "missing '(' after 'switch'");
        }
        let expr = self.parse_expr();
        if !self.expect_token_text(")") {
            self.err_custom_here("E3001", "unclosed condition, expected ')'");
        }
        if !self.cur_is_punct("{") {
            self.err_custom_here("E3005", "missing '{' to start switch-body");
        }
        self.expect_punct("{");

        let mut cases: Vec<Case> = vec![];
        let mut cur_body: Vec<Stmt> = vec![];
        let mut cur_label: Option<Expr> = None;
        let mut cur_span: Option<Span> = None;
        let mut has_label = false;

        loop {
            self.skip_trivia();
            if self.at_end() {
                self.err_custom_here("E3003", "unclosed switch, expected '}' before EOF");
                break;
            }
            if self.cur_is_punct("}") {
                if has_label {
                    let span = cur_span.unwrap_or_else(|| self.cur_span());
                    cases.push(Case {
                        label: cur_label.take(),
                        span,
                        body: std::mem::take(&mut cur_body),
                    });
                }
                self.bump();
                break;
            }
            if self.cur_is_kw("case") || self.cur_is_kw("default") {
                if has_label {
                    let span = cur_span.unwrap_or_else(|| self.cur_span());
                    cases.push(Case {
                        label: cur_label.take(),
                        span,
                        body: std::mem::take(&mut cur_body),
                    });
                    has_label = false;
                    cur_span = None;
                }
                if self.cur_is_kw("case") {
                    let kw_tok = self.bump().unwrap();
                    let span = kw_tok.span();
                    let v = self.parse_expr();
                    if !self.expect_token_text(":") {
                        self.err_custom_here("E2007", "missing ':' after case label");
                    }
                    cur_label = Some(v);
                    cur_span = Some(span);
                    has_label = true;
                } else {
                    let kw_tok = self.bump().unwrap();
                    let span = kw_tok.span();
                    if !self.expect_token_text(":") {
                        self.err_custom_here("E2007", "missing ':' after default");
                    }
                    cur_label = None;
                    cur_span = Some(span);
                    has_label = true;
                }
                continue;
            }
            let start_i = self.i;

            cur_body.push(self.parse_stmt());

            if self.i == start_i && !self.at_end() {

                let sp = self.cur_span();

                self.err_push(

                    "E9002",

                    "parser made no progress in switch; skipping tokens to recover".to_string(),

                    sp,

                );

                self.sync_in_switch();

                continue;

            }
        }
        Stmt::Switch { expr, cases }
    }

    fn parse_expr_stmt(&mut self) -> Stmt {
        let e = self.parse_expr();
        if !self.expect_token_text(";") {
            self.err_custom_here("E2002", "missing ';' after expression");
        }
        Stmt::ExprStmt(e)
    }

    /* ===================== 初始化器 ===================== */

    fn parse_initializer(&mut self) -> Init {
    if self.cur_is_punct("{") {
        let start_span = self.cur_span();
        self.bump(); // '{'

        let mut list = Vec::new();

        if !self.cur_is_punct("}") {
            loop {
                let before = self.i;

                list.push(self.parse_initializer());

                // If we didn't consume anything, recover to avoid infinite loops.
                if self.i == before && !self.at_end() {
                    self.err_custom_span(
                        "E4003",
                        "parser made no progress in initializer list; skipping tokens to recover"
                            .to_string(),
                        start_span,
                    );
                    // Try to sync to either ',' or '}' so we can continue parsing other inits.
                    while !self.at_end()
                        && !self.cur_is_punct(",")
                        && !self.cur_is_punct("}")
                    {
                        self.i += 1;
                    }
                }

                if self.cur_is_punct("}") {
                    break;
                }

                if self.cur_is_punct(",") {
                    self.bump(); // ','
                    // Allow trailing comma: { 1, 2, }
                    if self.cur_is_punct("}") {
                        break;
                    }
                    continue;
                } else {
                    self.err_custom_here("E4002", "missing ',' between initializers");
                    break;
                }
            }
        }

        if !self.cur_is_punct("}") {
            self.err_custom_here("E4001", "unclosed initializer list, expected '}'");
        }
        self.expect_punct("}");
        Init::List(list)
    } 
    pub fn parse_expr(&mut self) -> Expr {
        self.parse_expr_bp(0)
    }

    // Pratt parser (binding power)
    // Lowest -> highest:
    //   , (comma)
    //   assignment (=, +=, ...)
    //   ?: (ternary)
    //   ||, &&,
    //   |, ^, &,
    //   == !=,
    //   < <= > >=,
    //   << >>,
    //   + -,
    //   * / %,
    //   prefix, postfix
    fn parse_expr_bp(&mut self, min_bp: u8) -> Expr {
        self.skip_trivia();
        let mut lhs = self.parse_prefix();

        loop {
            self.skip_trivia();

            // Postfix operators (highest precedence)
            if self.cur_text_is("++") {
                self.bump();
                lhs = Expr::PostInc(Box::new(lhs));
                continue;
            }
            if self.cur_text_is("--") {
                self.bump();
                lhs = Expr::PostDec(Box::new(lhs));
                continue;
            }
            if self.cur_text_is("(") {
                let args = self.parse_call_args();
                lhs = Expr::CallExpr { callee: Box::new(lhs), args };
                continue;
            }
            if self.cur_text_is("[") {
                self.bump();
                let idx = self.parse_expr_bp(0);
                self.expect_punct("]");
                lhs = Expr::Index {
                    base: Box::new(lhs),
                    index: Box::new(idx),
                };
                continue;
            }
            if self.cur_text_is(".") {
                self.bump();
                let field = if self.cur_is(&TokenType::Identifier) {
                    self.bump().unwrap().text().to_string()
                } else {
                    self.err_custom_here("E3302", "expected field name after '.'");
                    "_field".into()
                };
                lhs = Expr::Member {
                    base: Box::new(lhs),
                    field,
                };
                continue;
            }
            if self.cur_text_is("->") {
                self.bump();
                let field = if self.cur_is(&TokenType::Identifier) {
                    self.bump().unwrap().text().to_string()
                } else {
                    self.err_custom_here("E3303", "expected field name after '->'");
                    "_field".into()
                };
                lhs = Expr::PtrMember {
                    base: Box::new(lhs),
                    field,
                };
                continue;
            }

            // Infix / ternary / assignment / comma
            let Some((op, l_bp, r_bp, kind)) = self.peek_infix_bp() else {
                break;
            };
            if l_bp < min_bp {
                break;
            }

            // consume operator token
            self.bump();

            match kind {
                InfixKind::Comma => {
                    let rhs = self.parse_expr_bp(r_bp);
                    lhs = match lhs {
                        Expr::Comma(mut v) => {
                            v.push(rhs);
                            Expr::Comma(v)
                        }
                        other => Expr::Comma(vec![other, rhs]),
                    };
                }
                InfixKind::Assign => {
                    self.skip_trivia();
                    if self.expr_stops_here() {
                        self.err_custom_here("E3312", &format!("expected expression after assignment operator '{}'", op));
                        break;
                    }
                    let rhs = self.parse_expr_bp(r_bp);
                    lhs = Expr::Assign {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                InfixKind::Ternary => {
                    // already consumed '?'
                    let then_e = self.parse_expr_bp(0);
                    if !self.cur_text_is(":") {
                        self.err_custom_here("E3310", "expected ':' in ternary expression");
                        // recovery: skip until ':' or a terminator
                        while !self.at_end() && !self.cur_text_is(":") && !self.expr_stops_here() {
                            self.i += 1;
                        }
                        if self.cur_text_is(":") {
                            self.bump();
                        } else {
                            // cannot recover ternary, keep lhs
                            break;
                        }
                    } else {
                        self.bump();
                    }
                    self.skip_trivia();
                    if self.expr_stops_here() {
                        self.err_custom_here("E3314", "expected expression after ':' in ternary expression");
                        break;
                    }
                    let else_e = self.parse_expr_bp(r_bp);
                    lhs = Expr::Ternary {
                        cond: Box::new(lhs),
                        then_e: Box::new(then_e),
                        else_e: Box::new(else_e),
                    };
                }
                InfixKind::Binary => {
                    self.skip_trivia();
                    if self.expr_stops_here() {
                        self.err_custom_here("E3313", &format!("expected expression after operator '{}'", op));
                        self.sync_expr();
                        break;
                    }
                    let rhs = self.parse_expr_bp(r_bp);
                    lhs = Expr::Binary {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
            }
        }

        lhs
    }

    
    /// True if the next token cannot start an expression (common terminators).
    fn expr_stops_here(&self) -> bool {
        if self.at_end() {
            return true;
        }

    /// Expression-level recovery: advance until an expression terminator, a statement start,
    /// or EOF. Does NOT consume the terminator.
    fn sync_expr(&mut self) {
        while !self.at_end() && !self.expr_stops_here() && !self.is_stmt_start() {
            self.bump();
            self.skip_trivia();
        }
    }

        let t = self.cur().unwrap();
        match t.kind() {
            TokenType::Punctuation => matches!(
                t.text(),
                ";" | ")" | "]" | "}" | "," | ":"
            ),
            TokenType::Keyword => matches!(t.text(), "case" | "default"),
            _ => false,
        }
    }

fn peek_infix_bp(&self) -> Option<(String, u8, u8, InfixKind)> {
        let t = self.cur()?;
        let op = t.text();

        // helper for left-assoc precedence p: (2p, 2p+1); right-assoc: (2p, 2p)
        fn left(p: u8) -> (u8, u8) {
            (p * 2, p * 2 + 1)
        }
        fn right(p: u8) -> (u8, u8) {
            (p * 2, p * 2)
        }

        // precedence levels (low -> high)
        const P_COMMA: u8 = 1;
        const P_ASSIGN: u8 = 2;
        const P_TERNARY: u8 = 3;
        const P_LOR: u8 = 4;
        const P_LAND: u8 = 5;
        const P_BOR: u8 = 6;
        const P_BXOR: u8 = 7;
        const P_BAND: u8 = 8;
        const P_EQ: u8 = 9;
        const P_REL: u8 = 10;
        const P_SHIFT: u8 = 11;
        const P_ADD: u8 = 12;
        const P_MUL: u8 = 13;

        match op {
            "," => {
                let (l, r) = left(P_COMMA);
                Some((",".into(), l, r, InfixKind::Comma))
            }

            // assignment (right associative)
            "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" => {
                let (l, r) = right(P_ASSIGN);
                Some((op.to_string(), l, r, InfixKind::Assign))
            }

            // ternary
            "?" => {
                let (l, r) = right(P_TERNARY);
                Some(("?".into(), l, r, InfixKind::Ternary))
            }

            "||" => {
                let (l, r) = left(P_LOR);
                Some(("||".into(), l, r, InfixKind::Binary))
            }
            "&&" => {
                let (l, r) = left(P_LAND);
                Some(("&&".into(), l, r, InfixKind::Binary))
            }
            "|" => {
                let (l, r) = left(P_BOR);
                Some(("|".into(), l, r, InfixKind::Binary))
            }
            "^" => {
                let (l, r) = left(P_BXOR);
                Some(("^".into(), l, r, InfixKind::Binary))
            }
            "&" => {
                let (l, r) = left(P_BAND);
                Some(("&".into(), l, r, InfixKind::Binary))
            }
            "==" | "!=" => {
                let (l, r) = left(P_EQ);
                Some((op.to_string(), l, r, InfixKind::Binary))
            }
            "<" | "<=" | ">" | ">=" => {
                let (l, r) = left(P_REL);
                Some((op.to_string(), l, r, InfixKind::Binary))
            }
            "<<" | ">>" => {
                let (l, r) = left(P_SHIFT);
                Some((op.to_string(), l, r, InfixKind::Binary))
            }
            "+" | "-" => {
                let (l, r) = left(P_ADD);
                Some((op.to_string(), l, r, InfixKind::Binary))
            }
            "*" | "/" | "%" => {
                let (l, r) = left(P_MUL);
                Some((op.to_string(), l, r, InfixKind::Binary))
            }
            _ => None,
        }
    }

    fn parse_prefix(&mut self) -> Expr {
        self.skip_trivia();

        // prefix ++/--
        if self.cur_text_is("++") {
            self.bump();
            return Expr::PreInc(Box::new(self.parse_prefix()));
        }
        if self.cur_text_is("--") {
            self.bump();
            return Expr::PreDec(Box::new(self.parse_prefix()));
        }

        // unary operators
        if let Some(t) = self.cur() {
            if matches!(t.kind(), TokenType::Operator) {
                let op = t.text();
                if ["+", "-", "!", "~", "&", "*"].contains(&op) {
                    let op_str = op.to_string();
                    self.bump();
                    return Expr::Unary {
                        op: op_str,
                        expr: Box::new(self.parse_prefix()),
                    };
                }
            }
        }

        // sizeof / alignof
        if self.cur_is_kw("sizeof") {
            let kw_span = self.cur_span();
            self.bump();
            self.skip_trivia();
            if self.cur_text_is("(") {
                let mark = self.save();
                self.bump(); // '('
                self.skip_trivia();
                if let Some(ty) = self.parse_type_name_full() {
                    self.skip_trivia();
                    if self.cur_text_is(")") {
                        self.bump();
                        return Expr::SizeofType(ty);
                    }
                }
                self.restore(mark);
            }
            // sizeof expr
            let e = self.parse_prefix();
            return Expr::SizeofExpr(Box::new(e));
        }

        if self.cur_is_kw("alignof") {
            let _kw_span = self.cur_span();
            self.bump();
            self.skip_trivia();
            if self.cur_text_is("(") {
                let mark = self.save();
                self.bump(); // '('
                self.skip_trivia();
                if let Some(ty) = self.parse_type_name_full() {
                    self.skip_trivia();
                    if self.cur_text_is(")") {
                        self.bump();
                        return Expr::AlignofType(ty);
                    }
                }
                self.restore(mark);
            }
            let e = self.parse_prefix();
            return Expr::AlignofExpr(Box::new(e));
        }

        // cast: (type) prefix
        if self.cur_text_is("(") {
            let mark = self.save();
            self.bump(); // '('
            self.skip_trivia();
            if let Some(ty) = self.parse_type_name_full() {
                self.skip_trivia();
                if self.cur_text_is(")") {
                    self.bump();
                    let e = self.parse_prefix();
                    return Expr::Cast {
                        ty,
                        expr: Box::new(e),
                    };
                }
            }
            self.restore(mark);
        }

        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Expr {
        self.skip_trivia();

        if self.cur_text_is("(") {
            self.bump();
            let e = self.parse_expr_bp(0);
            self.expect_punct(")");
            return e;
        }

        if let Some(t) = self.cur() {
            match t.kind() {
                TokenType::IntConstant => {
                    let s = t.text().to_string();
                    self.bump();
                    return Expr::Int(s);
                }
                TokenType::FloatConstant => {
                    let s = t.text().to_string();
                    self.bump();
                    return Expr::Float(s);
                }
                TokenType::StringLiteral => {
                    let s = t.text().to_string();
                    self.bump();
                    return Expr::Str(s);
                }
                TokenType::CharLiteral => {
                    let s = t.text().to_string();
                    self.bump();
                    return Expr::Char(s);
                }
                TokenType::Identifier => {
                    let s = t.text().to_string();
                    self.bump();
                    return Expr::Ident(s);
                }
                _ => {}
            }
        }

        self.err_custom_here("E3200", "expected expression");
        // 防止死循环：如果当前就是表达式终止符，就不要吞掉它，交给上层处理
        if !self.is_expr_end() && !self.at_end() {
            self.bump();
        }
        Expr::Ident("_error".into())
    }

    fn parse_call_args(&mut self) -> Vec<Expr> {
        // assumes current token is '('
        self.expect_punct("(");
        let mut args = Vec::new();
        self.skip_trivia();
        if self.cur_text_is(")") {
            self.bump();
            return args;
        }

        loop {
            let start_i = self.i;
            args.push(self.parse_expr_bp(0));
            self.skip_trivia();

            if self.cur_text_is(")") {
                self.bump();
                break;
            }

            if self.cur_text_is(",") {
                self.bump();
                self.skip_trivia();
                if self.cur_text_is(")") {
                    // allow trailing comma
                    self.bump();
                    break;
                }
            } else {
                self.err_custom_here("E3304", "expected ',' or ')' in argument list");
                // progress guard
                if self.i == start_i {
                    self.sync();
                }
                // best effort recovery: stop at ')'
                while !self.at_end() && !self.cur_text_is(")") {
                    self.i += 1;
                }
                if self.cur_text_is(")") {
                    self.bump();
                } else if self.at_end() {
                    self.err_expect_inserted("')'");
                }
                break;
            }
        }
        args
    }

    /* ============ 数组初始化器校验 ============ */

    fn dims_to_capacity(&self, dims: &[Option<String>]) -> Option<usize> {
        let mut cap: usize = 1;
        for d in dims {
            match d {
                Some(s) => {
                    if let Ok(v) = s.parse::<usize>() {
                        cap = cap.saturating_mul(v);
                    } else {
                        return None;
                    }
                }
                None => return None,
            }
        }
        Some(cap)
    }

    fn init_count_and_depth(init: &Init) -> (usize, usize) {
        match init {
            Init::Expr(_) => (1, 0),
            Init::List(list) => {
                let mut total = 0;
                let mut maxd = 0;
                for it in list {
                    let (c, d) = Self::init_count_and_depth(it);
                    total += c;
                    if d > maxd {
                        maxd = d;
                    }
                }
                (total, maxd + 1)
            }
        }
    }

    fn validate_array_initializer(&mut self, name: &str, name_span: Span, dims: &[Option<String>], init: &Init) {
        if dims.is_empty() {
            return;
        }
        let rank = dims.len();
        let (_cnt, depth) = Self::init_count_and_depth(init);

        if depth > rank {
            self.err_custom_span(
                "E4102",
                format!(
                    "initializer for '{}' has too many brace levels for an array of rank {}",
                    name, rank
                ),
                name_span,
            );
            return;
        }

        if let Some(cap) = self.dims_to_capacity(dims) {
            let (cnt, _) = Self::init_count_and_depth(init);
            if cnt > cap {
                self.err_custom_span(
                    "E4101",
                    format!(
                        "too many initializers for '{}': have {}, but capacity is {}",
                        name, cnt, cap
                    ),
                    name_span,
                );
            }
        }
    }

    /* ============ goto / label 一致性检查 ============ */

    fn check_labels_and_gotos(&mut self, items: &[Item]) {
        for it in items {
            if let Item::Function { body, .. } = it {
                let mut labels: Vec<(String, Span)> = Vec::new();
                let mut gotos: Vec<(String, Span)> = Vec::new();
                self.collect_labels_and_gotos_stmt(body, &mut labels, &mut gotos);

                for (gname, gspan) in &gotos {
                    if !labels.iter().any(|(lname, _)| lname == gname) {
                        self.err_custom_span(
                            "E2602",
                            format!("goto target label '{}' not defined in this function", gname),
                            *gspan,
                        );
                    }
                }

                let mut used: HashSet<&str> = HashSet::new();
                for (gname, _) in &gotos {
                    used.insert(gname.as_str());
                }
                for (lname, lspan) in &labels {
                    if !used.contains(lname.as_str()) {
                        self.err_custom_span(
                            "E2604",
                            format!("label '{}' declared but never used", lname),
                            *lspan,
                        );
                    }
                }
            }
        }
    }

    fn collect_labels_and_gotos_stmt(
        &mut self,
        stmt: &Stmt,
        labels: &mut Vec<(String, Span)>,
        gotos: &mut Vec<(String, Span)>,
    ) {
        match stmt {
            Stmt::Label { name, span, stmt } => {
                if labels.iter().any(|(n, _)| n == name) {
                    self.err_custom_span("E2603", format!("duplicate label '{}'", name), *span);
                }
                labels.push((name.clone(), *span));
                self.collect_labels_and_gotos_stmt(stmt, labels, gotos);
            }
            Stmt::Goto { name, span } => {
                gotos.push((name.clone(), *span));
            }
            Stmt::Block(stmts) => {
                for s in stmts {
                    self.collect_labels_and_gotos_stmt(s, labels, gotos);
                }
            }
            Stmt::If { then_branch, else_branch, .. } => {
                self.collect_labels_and_gotos_stmt(then_branch, labels, gotos);
                if let Some(e) = else_branch {
                    self.collect_labels_and_gotos_stmt(e, labels, gotos);
                }
            }
            Stmt::While { body, .. } => {
                self.collect_labels_and_gotos_stmt(body, labels, gotos);
            }
            Stmt::DoWhile { body, .. } => {
                self.collect_labels_and_gotos_stmt(body, labels, gotos);
            }
            Stmt::For { init, body, .. } => {
                if let Some(i) = init {
                    self.collect_labels_and_gotos_stmt(i, labels, gotos);
                }
                self.collect_labels_and_gotos_stmt(body, labels, gotos);
            }
            Stmt::Switch { cases, .. } => {
                for c in cases {
                    for s in &c.body {
                        self.collect_labels_and_gotos_stmt(s, labels, gotos);
                    }
                }
            }
            Stmt::VarDecl { .. }
            | Stmt::Return { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::ExprStmt(_)
            | Stmt::Empty => {}
        }
    }

    /* ============ break/continue 上下文检查 ============ */

    fn check_loops_and_breaks(&mut self, items: &[Item]) {
        for it in items {
            if let Item::Function { body, .. } = it {
                self.walk_loops_in_stmt(body, 0, 0);
            }
        }
    }

    fn walk_loops_in_stmt(&mut self, stmt: &Stmt, loop_depth: usize, switch_depth: usize) {
        match stmt {
            Stmt::Break(span) => {
                if loop_depth == 0 && switch_depth == 0 {
                    self.err_custom_span(
                        "E2701",
                        "break statement not within loop or switch".to_string(),
                        *span,
                    );
                }
            }
            Stmt::Continue(span) => {
                if loop_depth == 0 {
                    self.err_custom_span(
                        "E2702",
                        "continue statement not within loop".to_string(),
                        *span,
                    );
                }
            }
            Stmt::While { body, .. } => self.walk_loops_in_stmt(body, loop_depth + 1, switch_depth),
            Stmt::DoWhile { body, .. } => self.walk_loops_in_stmt(body, loop_depth + 1, switch_depth),
            Stmt::For { init, body, .. } => {
                if let Some(i) = init {
                    self.walk_loops_in_stmt(i, loop_depth, switch_depth);
                }
                self.walk_loops_in_stmt(body, loop_depth + 1, switch_depth);
            }
            Stmt::Switch { cases, .. } => {
                for c in cases {
                    for s in &c.body {
                        self.walk_loops_in_stmt(s, loop_depth, switch_depth + 1);
                    }
                }
            }
            Stmt::If { then_branch, else_branch, .. } => {
                self.walk_loops_in_stmt(then_branch, loop_depth, switch_depth);
                if let Some(e) = else_branch {
                    self.walk_loops_in_stmt(e, loop_depth, switch_depth);
                }
            }
            Stmt::Block(stmts) => {
                for s in stmts {
                    self.walk_loops_in_stmt(s, loop_depth, switch_depth);
                }
            }
            Stmt::Label { stmt, .. } => self.walk_loops_in_stmt(stmt, loop_depth, switch_depth),
            Stmt::VarDecl { .. }
            | Stmt::Return { .. }
            | Stmt::Goto { .. }
            | Stmt::ExprStmt(_)
            | Stmt::Empty => {}
        }
    }

    /* ============ return 语句简单类型检查 ============ */

    fn check_function_returns(&mut self, items: &[Item]) {
        for it in items {
            if let Item::Function { ret, ret_ptr, body, name_span, .. } = it {
                let is_void = *ret_ptr == 0 && ret.split_whitespace().any(|w| w == "void");
                self.check_returns_in_stmt(body, is_void);
                if !is_void {
                    let has_val = self.has_value_return(body);
                    if !has_val {
                        self.err_custom_span(
                            "E2803",
                            "non-void function may reach end without returning a value".to_string(),
                            *name_span,
                        );
                    }
                }
            }
        }
    }

    fn check_returns_in_stmt(&mut self, stmt: &Stmt, is_void: bool) {
        match stmt {
            Stmt::Return { value, span } => match (is_void, value.is_some()) {
                (true, true) => self.err_custom_span(
                    "E2801",
                    "void function should not return a value".to_string(),
                    *span,
                ),
                (false, false) => self.err_custom_span(
                    "E2802",
                    "non-void function should return a value".to_string(),
                    *span,
                ),
                _ => {}
            },
            Stmt::If { then_branch, else_branch, .. } => {
                self.check_returns_in_stmt(then_branch, is_void);
                if let Some(e) = else_branch {
                    self.check_returns_in_stmt(e, is_void);
                }
            }
            Stmt::While { body, .. } => self.check_returns_in_stmt(body, is_void),
            Stmt::DoWhile { body, .. } => self.check_returns_in_stmt(body, is_void),
            Stmt::For { init, body, .. } => {
                if let Some(i) = init {
                    self.check_returns_in_stmt(i, is_void);
                }
                self.check_returns_in_stmt(body, is_void);
            }
            Stmt::Switch { cases, .. } => {
                for c in cases {
                    for s in &c.body {
                        self.check_returns_in_stmt(s, is_void);
                    }
                }
            }
            Stmt::Block(stmts) => {
                for s in stmts {
                    self.check_returns_in_stmt(s, is_void);
                }
            }
            Stmt::Label { stmt, .. } => self.check_returns_in_stmt(stmt, is_void),
            Stmt::VarDecl { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Goto { .. }
            | Stmt::ExprStmt(_)
            | Stmt::Empty => {}
        }
    }

    fn has_value_return(&self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Return { value, .. } => value.is_some(),
            Stmt::If { then_branch, else_branch, .. } => {
                self.has_value_return(then_branch)
                    || else_branch.as_ref().map_or(false, |e| self.has_value_return(e))
            }
            Stmt::While { body, .. } => self.has_value_return(body),
            Stmt::DoWhile { body, .. } => self.has_value_return(body),
            Stmt::For { init, body, .. } => {
                init.as_ref().map_or(false, |i| self.has_value_return(i)) || self.has_value_return(body)
            }
            Stmt::Switch { cases, .. } => {
                for c in cases {
                    for s in &c.body {
                        if self.has_value_return(s) {
                            return true;
                        }
                    }
                }
                false
            }
            Stmt::Block(stmts) => stmts.iter().any(|s| self.has_value_return(s)),
            Stmt::Label { stmt, .. } => self.has_value_return(stmt),
            Stmt::VarDecl { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Goto { .. }
            | Stmt::ExprStmt(_)
            | Stmt::Empty => false,
        }
    }

    /* ============ switch case/default 语义检查 ============ */

    fn check_switch_cases(&mut self, items: &[Item]) {
        for it in items {
            match it {
                Item::Function { body, .. } => self.walk_switch_cases(body),
                Item::Global(stmt) => self.walk_switch_cases(stmt),
                Item::StructDef { .. } | Item::EnumDef { .. } => {}
            }
        }
    }

    fn walk_switch_cases(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Switch { cases, .. } => {
                self.check_one_switch_cases(cases);
                for c in cases {
                    for s in &c.body {
                        self.walk_switch_cases(s);
                    }
                }
            }
            Stmt::If { then_branch, else_branch, .. } => {
                self.walk_switch_cases(then_branch);
                if let Some(e) = else_branch {
                    self.walk_switch_cases(e);
                }
            }
            Stmt::While { body, .. } => self.walk_switch_cases(body),
            Stmt::DoWhile { body, .. } => self.walk_switch_cases(body),
            Stmt::For { init, body, .. } => {
                if let Some(i) = init {
                    self.walk_switch_cases(i);
                }
                self.walk_switch_cases(body);
            }
            Stmt::Block(stmts) => {
                for s in stmts {
                    self.walk_switch_cases(s);
                }
            }
            Stmt::Label { stmt, .. } => self.walk_switch_cases(stmt),
            Stmt::VarDecl { .. }
            | Stmt::Return { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Goto { .. }
            | Stmt::ExprStmt(_)
            | Stmt::Empty => {}
        }
    }

    fn check_one_switch_cases(&mut self, cases: &[Case]) {
        let mut default_span: Option<Span> = None;
        let mut seen_cases: HashMap<String, Span> = HashMap::new();

        for c in cases {
            match &c.label {
                None => {
                    if default_span.is_some() {
                        self.err_custom_span(
                            "E2901",
                            "duplicate default label in switch".to_string(),
                            c.span,
                        );
                    } else {
                        default_span = Some(c.span);
                    }
                }
                Some(expr) => {
                    let key_opt = match expr {
                        Expr::Int(v) | Expr::Char(v) | Expr::Str(v) => Some(v.clone()),
                        _ => None,
                    };
                    if let Some(key) = key_opt {
                        if seen_cases.contains_key(&key) {
                            self.err_custom_span(
                                "E2902",
                                format!("duplicate case label '{}' in switch", key),
                                c.span,
                            );
                        } else {
                            seen_cases.insert(key, c.span);
                        }
                    }
                }
            }
        }

        // fallthrough 检查：前一个 case 的最后一条非空语句如果不是“终止”，则认为隐式 fallthrough
        if cases.len() >= 2 {
            for i in 0..cases.len() - 1 {
                let c = &cases[i];

                let last_stmt = c.body.iter().rev().find(|st| !matches!(st, Stmt::Empty));
                if let Some(last) = last_stmt {
                    if !self.stmt_definitely_breaks(last) {
                        self.err_custom_span(
                            "E2905",
                            "implicit fallthrough from this case to the next case/default".to_string(),
                            c.span,
                        );
                    }
                }
            }
        }
    }

    /// 在 switch 的语境里，判断一条语句是否“肯定终止”当前 case：
    /// 我们认为 break/return/goto/continue 以及某些组合结构是终止的。
    fn stmt_definitely_breaks(&self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Return { .. }
            | Stmt::Goto { .. } => true,

            Stmt::Block(stmts) => {
                if let Some(last) = stmts.iter().rev().find(|s| !matches!(s, Stmt::Empty)) {
                    self.stmt_definitely_breaks(last)
                } else {
                    false
                }
            }

            Stmt::If { then_branch, else_branch, .. } => {
                if let Some(e) = else_branch {
                    self.stmt_definitely_breaks(then_branch) && self.stmt_definitely_breaks(e)
                } else {
                    false
                }
            }

            Stmt::Label { stmt, .. } => self.stmt_definitely_breaks(stmt),

            Stmt::While { .. }
            | Stmt::DoWhile { .. }
            | Stmt::For { .. }
            | Stmt::Switch { .. }
            | Stmt::VarDecl { .. }
            | Stmt::ExprStmt(_)
            | Stmt::Empty => false,
        }
    }

    /* ============ 函数重定义检查 ============ */

    fn check_function_redefinitions(&mut self, items: &[Item]) {
        let mut seen: HashMap<String, Span> = HashMap::new();
        for it in items {
            if let Item::Function { name, name_span, .. } = it {
                if seen.contains_key(name) {
                    self.err_custom_span(
                        "E5501",
                        format!("redefinition of function '{}'", name),
                        *name_span,
                    );
                } else {
                    seen.insert(name.clone(), *name_span);
                }
            }
        }
    }

    /* ============ 不可达代码检查（增强版） ============ */

    fn check_unreachable(&mut self, items: &[Item]) {
        for it in items {
            if let Item::Function { body, .. } = it {
                self.walk_unreachable_stmt(body);
            }
        }
    }

    fn walk_unreachable_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Block(stmts) => self.walk_unreachable_block(stmts),
            Stmt::If { then_branch, else_branch, .. } => {
                self.walk_unreachable_stmt(then_branch);
                if let Some(e) = else_branch {
                    self.walk_unreachable_stmt(e);
                }
            }
            Stmt::While { body, .. } => self.walk_unreachable_stmt(body),
            Stmt::DoWhile { body, .. } => self.walk_unreachable_stmt(body),
            Stmt::For { init, body, .. } => {
                if let Some(i) = init {
                    self.walk_unreachable_stmt(i);
                }
                self.walk_unreachable_stmt(body);
            }
            Stmt::Switch { cases, .. } => {
                for c in cases {
                    for s in &c.body {
                        self.walk_unreachable_stmt(s);
                    }
                }
            }
            Stmt::Label { stmt, .. } => self.walk_unreachable_stmt(stmt),

            Stmt::VarDecl { .. }
            | Stmt::Return { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Goto { .. }
            | Stmt::ExprStmt(_)
            | Stmt::Empty => {}
        }
    }

    fn walk_unreachable_block(&mut self, stmts: &[Stmt]) {
        let len = stmts.len();
        for i in 0..len {
            let stmt = &stmts[i];

            // 先递归检查子结构内部
            self.walk_unreachable_stmt(stmt);

            // 如果当前语句保证返回，那么后面的语句不可达
            if self.stmt_definitely_exits_function(stmt) && i + 1 < len {
                // 尽量指向第一条不可达语句（如果能取到 span）
                let mut report_span = self.stmt_span_for_unreachable(stmt);
                for j in (i + 1)..len {
                    if !matches!(stmts[j], Stmt::Empty) {
                        if let Some(sp) = self.stmt_span_best_effort(&stmts[j]) {
                            report_span = sp;
                        }
                        break;
                    }
                }

                self.err_custom_span(
                    "E5601",
                    "unreachable code: statement will never be executed".to_string(),
                    report_span,
                );
                break;
            }
        }
    }

    fn stmt_definitely_exits_function(&self, stmt: &Stmt) -> bool {
        self.stmt_guaranteed_return(stmt)
    }

    fn stmt_guaranteed_return(&self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Return { .. } => true,

            Stmt::Block(stmts) => {
                // 顺序执行：遇到一个“保证 return”的语句，后续不再执行
                for s in stmts {
                    if self.stmt_guaranteed_return(s) {
                        return true;
                    }
                }
                false
            }

            Stmt::If { then_branch, else_branch, .. } => {
                if let Some(e) = else_branch {
                    self.stmt_guaranteed_return(then_branch) && self.stmt_guaranteed_return(e)
                } else {
                    false
                }
            }

            Stmt::Switch { cases, .. } => {
                // 简化规则：必须有 default；并且每个 case 的最后一条非空语句都保证 return
                let mut has_default = false;
                for c in cases {
                    if c.label.is_none() {
                        has_default = true;
                        break;
                    }
                }
                if !has_default {
                    return false;
                }
                for c in cases {
                    let last_non_empty = c.body.iter().rev().find(|s| !matches!(s, Stmt::Empty));
                    match last_non_empty {
                        Some(s) if self.stmt_guaranteed_return(s) => {}
                        _ => return false,
                    }
                }
                true
            }

            Stmt::Label { stmt, .. } => self.stmt_guaranteed_return(stmt),

            Stmt::While { .. }
            | Stmt::DoWhile { .. }
            | Stmt::For { .. }
            | Stmt::VarDecl { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Goto { .. }
            | Stmt::ExprStmt(_)
            | Stmt::Empty => false,
        }
    }

    fn stmt_span_for_unreachable(&self, stmt: &Stmt) -> Span {
        match stmt {
            Stmt::Return { span, .. } => *span,
            Stmt::Break(sp) => *sp,
            Stmt::Continue(sp) => *sp,
            Stmt::Goto { span, .. } => *span,
            Stmt::Label { span, .. } => *span,
            _ => Span { line: 1, col: 1, idx: 0, len: 1 },
        }
    }

    fn stmt_span_best_effort(&self, stmt: &Stmt) -> Option<Span> {
        match stmt {
            Stmt::Return { span, .. } => Some(*span),
            Stmt::Break(sp) => Some(*sp),
            Stmt::Continue(sp) => Some(*sp),
            Stmt::Goto { span, .. } => Some(*span),
            Stmt::Label { span, .. } => Some(*span),
            // 这些语句目前 AST 没存 span，只能返回 None
            Stmt::If { .. }
            | Stmt::While { .. }
            | Stmt::For { .. }
            | Stmt::Switch { .. }
            | Stmt::DoWhile { .. }
            | Stmt::VarDecl { .. }
            | Stmt::ExprStmt(_)
            | Stmt::Block(_)
            | Stmt::Empty => None,
        }
    }
}

/* ===================== 打印器（含 Init / Struct / Enum） ===================== */

pub fn stringify_items(items: &[Item]) -> String {
    fn indent(n: usize) -> String {
        "  ".repeat(n)
    }
    fn stars(n: usize) -> String {
        "*".repeat(n)
    }
    fn fmt_dims(dims: &[Option<String>]) -> String {
        let mut s = String::new();
        for d in dims {
            s.push('[');
            if let Some(v) = d {
                s.push_str(v);
            }
            s.push(']');
        }
        s
    }
    fn fmt_ctype(t: &CType) -> String {
        if t.ptr > 0 {
            format!("{} {}", t.base, "*".repeat(t.ptr))
        } else {
            t.base.clone()
        }
    }

    fn fmt_expr(e: &Expr, _d: usize, out: &mut String) {
        match e {
            Expr::Int(v) | Expr::Float(v) | Expr::Str(v) | Expr::Char(v) | Expr::Ident(v) => out.push_str(v),
            Expr::Unary { op, expr } => {
                out.push('(');
                out.push_str(op);
                out.push(' ');
                fmt_expr(expr, 0, out);
                out.push(')');
            }
            Expr::Binary { op, lhs, rhs } => {
                out.push('(');
                fmt_expr(lhs, 0, out);
                out.push_str(&format!(" {} ", op));
                fmt_expr(rhs, 0, out);
                out.push(')');
            }
            Expr::CallExpr { callee, args } => {
                fmt_expr(callee, 0, out);
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    fmt_expr(a, 0, out);
                }
                out.push(')');
            }
            Expr::Assign { op, lhs, rhs } => {
                out.push('(');
                fmt_expr(lhs, 0, out);
                out.push_str(&format!(" {} ", op));
                fmt_expr(rhs, 0, out);
                out.push(')');
            }
            Expr::Ternary { cond, then_e, else_e } => {
                out.push('(');
                fmt_expr(cond, 0, out);
                out.push_str(" ? ");
                fmt_expr(then_e, 0, out);
                out.push_str(" : ");
                fmt_expr(else_e, 0, out);
                out.push(')');
            }
            Expr::PostInc(x) => {
                fmt_expr(x, 0, out);
                out.push_str("++");
            }
            Expr::PostDec(x) => {
                fmt_expr(x, 0, out);
                out.push_str("--");
            }
            Expr::PreInc(x) => {
                out.push_str("(++ ");
                fmt_expr(x, 0, out);
                out.push(')');
            }
            Expr::PreDec(x) => {
                out.push_str("(-- ");
                fmt_expr(x, 0, out);
                out.push(')');
            }
            Expr::Index { base, index } => {
                fmt_expr(base, 0, out);
                out.push('[');
                fmt_expr(index, 0, out);
                out.push(']');
            }
            Expr::Member { base, field } => {
                fmt_expr(base, 0, out);
                out.push('.');
                out.push_str(field);
            }
            Expr::PtrMember { base, field } => {
                fmt_expr(base, 0, out);
                out.push_str("->");
                out.push_str(field);
            }
            Expr::Comma(list) => {
                out.push('(');
                for (i, ee) in list.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    fmt_expr(ee, 0, out);
                }
                out.push(')');
            }
            Expr::Cast { ty, expr } => {
                out.push_str("((");
                out.push_str(&fmt_ctype(ty));
                out.push_str(") ");
                fmt_expr(expr, 0, out);
                out.push(')');
            }
            Expr::SizeofExpr(x) => {
                out.push_str("sizeof ");
                fmt_expr(x, 0, out);
            }
            Expr::SizeofType(t) => {
                out.push_str("sizeof(");
                out.push_str(&fmt_ctype(t));
                out.push(')');
            }
            Expr::AlignofExpr(x) => {
                out.push_str("alignof ");
                fmt_expr(x, 0, out);
            }
            Expr::AlignofType(t) => {
                out.push_str("alignof(");
                out.push_str(&fmt_ctype(t));
                out.push(')');
            }
        }
    }

    fn fmt_init(init: &Init, d: usize, out: &mut String) {
        match init {
            Init::Expr(e) => fmt_expr(e, d, out),
            Init::List(list) => {
                out.push('{');
                for (i, it) in list.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    fmt_init(it, d, out);
                }
                out.push('}');
            }
        }
    }

    fn fmt_decl_line(
        prefix: &str,
        ty: &str,
        ptr: usize,
        name: &str,
        array_dims: &[Option<String>],
        init: &Option<Init>,
        d: usize,
        out: &mut String,
    ) {
        out.push_str(&format!(
            "{}{} {}{}",
            indent(d),
            prefix,
            ty,
            if ptr > 0 {
                format!(" {}", stars(ptr))
            } else {
                "".into()
            }
        ));
        out.push(' ');
        out.push_str(name);
        out.push_str(&fmt_dims(array_dims));
        if let Some(i) = init {
            out.push_str(" = ");
            fmt_init(i, d, out);
        }
        out.push('\n');
    }

    fn fmt_stmt(s: &Stmt, d: usize, out: &mut String) {
        match s {
            Stmt::VarDecl { ty, ptr, name, array_dims, init } => {
                fmt_decl_line("decl", ty, *ptr, name, array_dims, init, d, out);
            }
            Stmt::Return { value, .. } => {
                out.push_str(&format!("{}return", indent(d)));
                if let Some(e) = value {
                    out.push(' ');
                    fmt_expr(e, d, out);
                }
                out.push('\n');
            }
            Stmt::If { cond, then_branch, else_branch } => {
                out.push_str(&format!("{}if ", indent(d)));
                fmt_expr(cond, d, out);
                out.push('\n');
                fmt_stmt(then_branch, d + 1, out);
                if let Some(el) = else_branch {
                    out.push_str(&format!("{}else\n", indent(d)));
                    fmt_stmt(el, d + 1, out);
                }
            }
            Stmt::While { cond, body } => {
                out.push_str(&format!("{}while ", indent(d)));
                fmt_expr(cond, d, out);
                out.push('\n');
                fmt_stmt(body, d + 1, out);
            }
            Stmt::DoWhile { body, cond } => {
                out.push_str(&format!("{}do\n", indent(d)));
                fmt_stmt(body, d + 1, out);
                out.push_str(&format!("{}while ", indent(d)));
                fmt_expr(cond, d, out);
                out.push('\n');
            }
            Stmt::For { init, cond, step, body } => {
                out.push_str(&format!("{}for (", indent(d)));
                if let Some(i) = init {
                    fmt_stmt(i, d + 1, out);
                } else {
                    out.push_str("; ");
                }
                if let Some(c) = cond {
                    fmt_expr(c, d, out);
                }
                out.push_str("; ");
                if let Some(st) = step {
                    fmt_expr(st, d, out);
                }
                out.push_str(")\n");
                fmt_stmt(body, d + 1, out);
            }
            Stmt::Switch { expr, cases } => {
                out.push_str(&format!("{}switch ", indent(d)));
                fmt_expr(expr, d, out);
                out.push_str(" {\n");
                for c in cases {
                    match &c.label {
                        Some(e) => {
                            out.push_str(&format!("{}  case ", indent(d)));
                            fmt_expr(e, d, out);
                            out.push_str(":\n");
                        }
                        None => {
                            out.push_str(&format!("{}  default:\n", indent(d)));
                        }
                    }
                    for st in &c.body {
                        fmt_stmt(st, d + 2, out);
                    }
                }
                out.push_str(&format!("{}}}\n", indent(d)));
            }
            Stmt::Break(_) => out.push_str(&format!("{}break\n", indent(d))),
            Stmt::Continue(_) => out.push_str(&format!("{}continue\n", indent(d))),
            Stmt::Goto { name, .. } => out.push_str(&format!("{}goto {}\n", indent(d), name)),
            Stmt::Label { name, stmt, .. } => {
                out.push_str(&format!("{}label {}:\n", indent(d), name));
                fmt_stmt(stmt, d + 1, out);
            }
            Stmt::ExprStmt(e) => {
                out.push_str(&format!("{}expr ", indent(d)));
                fmt_expr(e, d, out);
                out.push('\n');
            }
            Stmt::Block(v) => {
                out.push_str(&format!("{}block {{\n", indent(d)));
                for st in v {
                    fmt_stmt(st, d + 1, out);
                }
                out.push_str(&format!("{}}}\n", indent(d)));
            }
            Stmt::Empty => out.push_str(&format!("{};\n", indent(d))),
        }
    }

    let mut s = String::new();
    for it in items {
        match it {
            Item::Function { ret, ret_ptr, name, name_span: _, params, body } => {
                s.push_str(&format!(
                    "fn {}{} {}(",
                    ret,
                    if *ret_ptr > 0 { format!(" {}", "*".repeat(*ret_ptr)) } else { "".into() },
                    name
                ));
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    let dims = {
                        let mut tmp = String::new();
                        for d in &p.array_dims {
                            tmp.push('[');
                            if let Some(v) = d {
                                tmp.push_str(v);
                            }
                            tmp.push(']');
                        }
                        tmp
                    };
                    s.push_str(&format!(
                        "{}{} {}{}",
                        p.ty,
                        if p.ptr > 0 { format!(" {}", "*".repeat(p.ptr)) } else { "".into() },
                        p.name,
                        dims
                    ));
                }
                s.push_str(")\n");
                fmt_stmt(body, 1, &mut s);
            }
            Item::Global(g) => {
                s.push_str("global ");
                fmt_stmt(g, 0, &mut s);
            }
            Item::StructDef { kind, name, fields } => {
                let kind_str = match kind {
                    StructKind::Struct => "struct",
                    StructKind::Union => "union",
                };
                s.push_str(&format!("{} {} {{\n", kind_str, name));
                for f in fields {
                    s.push_str("  field ");
                    s.push_str(&f.ty);
                    if f.ptr > 0 {
                        s.push(' ');
                        s.push_str(&"*".repeat(f.ptr));
                    }
                    s.push(' ');
                    s.push_str(&f.name);
                    s.push_str(&fmt_dims(&f.array_dims));
                    if let Some(bw) = &f.bit_width {
                        s.push_str(" : ");
                        fmt_expr(bw, 0, &mut s);
                    }
                    s.push('\n');
                }
                s.push_str("}\n");
            }
            Item::EnumDef { name, consts } => {
                s.push_str(&format!("enum {} {{\n", name));
                for c in consts {
                    s.push_str("  ");
                    s.push_str(&c.name);
                    if let Some(v) = &c.value {
                        s.push_str(" = ");
                        fmt_expr(v, 0, &mut s);
                    }
                    s.push('\n');
                }
                s.push_str("}\n");
            }
        }
    }
    s
}
