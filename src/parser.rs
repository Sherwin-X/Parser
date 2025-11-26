use crate::token::{Token, TokenType, Span};
use std::collections::HashSet;

/* ===================== AST ===================== */

#[derive(Debug, Clone)]
pub enum Expr {
    Int(String),
    Float(String),
    Str(String),
    Char(String),
    Ident(String),
    Binary { op: String, lhs: Box<Expr>, rhs: Box<Expr> },
    Unary  { op: String, expr: Box<Expr> },      // + - ! ~ & *
    Call   { callee: String, args: Vec<Expr> },
    Assign { op: String, lhs: Box<Expr>, rhs: Box<Expr> },
    Ternary { cond: Box<Expr>, then_e: Box<Expr>, else_e: Box<Expr> },
    PostInc(Box<Expr>),
    PostDec(Box<Expr>),
    PreInc(Box<Expr>),
    PreDec(Box<Expr>),
    Index  { base: Box<Expr>, index: Box<Expr> },
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
pub enum Stmt {
    VarDecl {
        ty: String,
        ptr: usize,
        name: String,
        array_dims: Vec<Option<String>>,
        init: Option<Init>,
    },
    Return(Option<Expr>),
    If     { cond: Expr, then_branch: Box<Stmt>, else_branch: Option<Box<Stmt>> },
    While  { cond: Expr, body: Box<Stmt> },
    For    { init: Option<Box<Stmt>>, cond: Option<Expr>, step: Option<Expr>, body: Box<Stmt> },
    Switch { expr: Expr, cases: Vec<Case> },
    Break,
    Continue,
    ExprStmt(Expr),
    Block(Vec<Stmt>),
    Empty,
}

#[derive(Debug, Clone)]
pub struct Case {
    pub label: Option<Expr>, // None = default
    pub body: Vec<Stmt>,
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
    Function { ret: String, ret_ptr: usize, name: String, params: Vec<Param>, body: Stmt },
    Global(Stmt),
    StructDef { kind: StructKind, name: String, fields: Vec<StructField> },
    EnumDef   { name: String, consts: Vec<EnumConst> },
}

/* ===================== 错误结构（带 caret） ===================== */

#[derive(Debug, Clone)]
pub struct ParseError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub line_text: String,
}

impl ParseError {
    pub fn render(&self) -> String {
        let mut caret = String::new();
        let pos = self.span.col.saturating_sub(1);
        caret.push_str(&" ".repeat(pos));
        caret.push_str(&"^".repeat(self.span.len.max(1)));
        format!(
"{}: {} at {}:{}\n{}\n{}\n",
            self.code, self.message, self.span.line, self.span.col, self.line_text, caret
        )
    }
}

/* ===================== Parser ===================== */

pub struct Parser {
    tokens: Vec<Token>,
    i: usize,
    source: String,
    pub errors: Vec<ParseError>,

    // typedef 符号表（只存名字，不展开真实类型）
    typedefs: HashSet<String>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, source: String) -> Self {
        Self {
            tokens,
            i: 0,
            source,
            errors: vec![],
            typedefs: HashSet::new(),
        }
    }

    #[inline] fn at_end(&self)->bool{ self.i>=self.tokens.len() }
    #[inline] fn cur(&self)->Option<&Token>{ self.tokens.get(self.i) }
    #[inline] fn cur_is(&self, k: &TokenType)->bool{ self.cur().map(|t| t.kind()==k).unwrap_or(false) }
    #[inline] fn cur_is_kw(&self, kw: &str)->bool{ self.cur().map(|t| matches!(t.kind(), TokenType::Keyword) && t.text()==kw).unwrap_or(false) }
    #[inline] fn cur_is_punct(&self, ch: &str)->bool{ self.cur().map(|t| matches!(t.kind(), TokenType::Punctuation) && t.text()==ch).unwrap_or(false) }
    #[inline] fn cur_is_op(&self, op: &str)->bool{ self.cur().map(|t| matches!(t.kind(), TokenType::Operator) && t.text()==op).unwrap_or(false) }
    fn bump(&mut self)->Option<Token>{ if self.at_end(){None}else{ let t=self.tokens[self.i].clone(); self.i+=1; Some(t) } }

    fn save(&self)->usize { self.i }
    fn restore(&mut self, mark: usize) { self.i = mark; }

    fn peek_prev_span(&self) -> Span {
        if self.i > 0 { self.tokens[self.i-1].span() } else { Span{ line:1, col:1, idx:0, len:0 } }
    }
    fn cur_span(&self) -> Span {
        if let Some(t)=self.cur() { t.span() } else { self.peek_prev_span() }
    }

    fn line_text_at(&self, span: Span) -> String {
        let bs = self.source.as_bytes();
        let mut cur = 1usize; let mut start = 0usize;
        for i in 0..=bs.len() {
            if i == bs.len() || bs[i] == b'\n' {
                if cur == span.line { return String::from_utf8_lossy(&bs[start..i]).into_owned(); }
                cur += 1; start = i + 1;
            }
        }
        String::new()
    }

    fn err_push(&mut self, code: &'static str, message: String, span: Span) {
        let line_text = self.line_text_at(span);
        self.errors.push(ParseError{ code, message, span, line_text });
    }

    fn err_expect(&mut self, expected: &str) {
        let span = self.cur_span();
        let got = if self.at_end(){ "EOF".to_string() } else { format!("'{}'", self.tokens[self.i].text()) };
        self.err_push("E1001", format!("expected {}, found {}", expected, got), span);
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

    fn expect_punct(&mut self, ch: &str){
        if !self.cur_is_punct(ch){ self.err_expect(&format!("'{}'", ch)); } else { self.bump(); }
    }
    fn expect_kw(&mut self, kw: &str){
        if !self.cur_is_kw(kw){ self.err_expect(&format!("keyword '{}'", kw)); } else { self.bump(); }
    }
    fn expect_token_text(&mut self, text: &str) -> bool {
        if self.cur_is_punct(text) || self.cur_is_op(text) || self.cur_is_kw(text) {
            self.bump(); true
        } else { self.err_expect(&format!("'{}'", text)); false }
    }

    // 同步：到分号/右花/右括/右方/逗号/下一 case/default
    fn sync(&mut self){
        while !self.at_end() {
            if self.cur_is_punct(";") || self.cur_is_punct("}") || self.cur_is_punct(")")
               || self.cur_is_punct("]") || self.cur_is_punct(",")
               || self.cur_is_kw("case") || self.cur_is_kw("default") {
                return;
            }
            self.i += 1;
        }
    }

    fn skip_trivia(&mut self){
        while let Some(t)=self.cur() {
            if matches!(t.kind(), TokenType::Whitespace | TokenType::Comment | TokenType::Preprocessor) { self.i+=1; } else { break; }
        }
    }

    /* ===================== 类型关键字 / typedef 辅助 ===================== */

    // 内建类型关键字 + const/volatile 作为基本 type specifier 的一部分
    fn is_builtin_type_kw_token(t: &Token) -> bool {
        if !matches!(t.kind(), TokenType::Keyword) { return false; }
        matches!(
            t.text(),
            "void" | "char" | "short" | "int" | "long" |
            "signed" | "unsigned" | "float" | "double" |
            "const" | "volatile"
        )
    }

    // 存储类别 / 函数说明符：static / extern / auto / register / inline / _Thread_local
    fn is_storage_or_func_spec_kw(t: &Token) -> bool {
        if !matches!(t.kind(), TokenType::Keyword) { return false; }
        matches!(
            t.text(),
            "static" | "extern" | "auto" | "register" | "inline" | "_Thread_local"
        )
    }

    // 指针上的修饰符：const / volatile / restrict
    fn is_ptr_qualifier_kw(t: &Token) -> bool {
        if !matches!(t.kind(), TokenType::Keyword) { return false; }
        matches!(t.text(), "const" | "volatile" | "restrict")
    }

    // 是否是 struct/union/enum 类型名字（字符串形式）
    fn is_tag_type_name(name: &str) -> bool {
        name.starts_with("struct ") || name.starts_with("union ") || name.starts_with("enum ")
    }

    // 当前 token 是否可以开始一个 "类型"（内建组合 / typedef 名 / struct/union/enum 标签类型）
    // 这里要忽略前面的 storage-class / inline 等说明符
    fn peek_type_start(&self) -> bool {
        let mut j = self.i;
        while j < self.tokens.len() {
            let t = &self.tokens[j];

            // 跳过 trivia
            if matches!(t.kind(), TokenType::Whitespace | TokenType::Comment | TokenType::Preprocessor) {
                j += 1;
                continue;
            }

            // 跳过存储/函数说明符
            if Self::is_storage_or_func_spec_kw(t) {
                j += 1;
                continue;
            }

            // 真正判断类型起始
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

    // 解析一串内建类型关键字：例如 "unsigned long int", "const int"
    fn parse_builtin_type_keyword_seq(&mut self) -> Option<Vec<String>> {
        if !self.cur().map(Self::is_builtin_type_kw_token).unwrap_or(false) {
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

    /// 在类型上下文中，跳过一个 `{ ... }` block，用于内联/匿名 struct/union/enum 定义
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

    // 声明里的「基础类型」（不含 *），可以是内建组合 / typedef 名 / struct/union/enum Tag 或内联匿名定义
    fn parse_decl_base_type(&mut self) -> Option<String> {
        // 先吃掉前面的存储/函数说明符（static / extern / inline 等）
        loop {
            if let Some(t) = self.cur() {
                if Self::is_storage_or_func_spec_kw(t) {
                    self.bump();
                    continue;
                }
            }
            break;
        }

        // 1) 内建组合
        if let Some(specs) = self.parse_builtin_type_keyword_seq() {
            return Some(Self::specs_to_string(&specs));
        }
        // 2) typedef 名
        if let Some(t) = self.cur() {
            if matches!(t.kind(), TokenType::Identifier) && self.typedefs.contains(t.text()) {
                let name = t.text().to_string();
                self.bump();
                return Some(name);
            }
        }
        // 3) struct / union / enum 标签类型 或 内联匿名定义
        if self.cur_is_kw("struct") || self.cur_is_kw("union") || self.cur_is_kw("enum") {
            let kw_tok = self.bump().unwrap();
            let kw = kw_tok.text().to_string();

            // 内联/匿名：struct { ... } [name] ...
            if self.cur_is_punct("{") {
                self.skip_brace_block_in_type();
                let base = format!("{} <anon>", kw);
                return Some(base);
            }

            // 正常：struct Tag
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

    // 类型名（含 *），用于 cast / sizeof(type) / alignof(type)，也支持内联匿名 struct/union/enum
    fn parse_type_name_full(&mut self) -> Option<CType> {
        // 1. 尝试内建组合
        let mark = self.save();
        if let Some(specs) = self.parse_builtin_type_keyword_seq() {
            let base = Self::specs_to_string(&specs);
            let ptr = self.parse_pointer_stars();
            return Some(CType { base, ptr });
        }
        self.restore(mark);

        // 2. 尝试 typedef 名
        if let Some(t) = self.cur() {
            if matches!(t.kind(), TokenType::Identifier) && self.typedefs.contains(t.text()) {
                let base = t.text().to_string();
                self.bump();
                let ptr = self.parse_pointer_stars();
                return Some(CType { base, ptr });
            }
        }

        // 3. 尝试 struct / union / enum 标签类型 或 内联匿名定义
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
            if self.at_end(){ break; }

            // 1) typedef 声明：不会生成 Item，只更新 typedef 集合
            if self.cur_is_kw("typedef") {
                self.parse_typedef_decl();
                continue;
            }

            // 2) struct / union 顶层定义：struct Point { ... };
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
                    // 不是定义（可能是类型在声明里用），回退交给普通声明路径
                    self.restore(mark);
                }
            }

            // 3) enum 顶层定义：enum Color { RED, GREEN, ... };
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
                    // 不是定义（只是类型说明符），回退交给普通声明路径
                    self.restore(mark);
                }
            }

            // 4) 正常的函数 / 全局变量声明
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
                        items.push(Item::Function{ ret: base_ty, ret_ptr, name, params, body });
                    } else {
                        // 全局变量声明
                        let first_dims = self.parse_array_dims_multi();
                        let first_init = if self.cur_is_op("=") { self.bump(); Some(self.parse_initializer()) } else { None };
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
                                let ini  = if self.cur_is_op("=") { self.bump(); Some(self.parse_initializer()) } else { None };
                                decls.push((ptr, nm, nm_span, dims, ini));
                            } else { self.err_custom_here("E2102", "expected identifier after ',' in declaration"); break; }
                        }
                        if !self.expect_token_text(";"){ self.err_custom_here("E2001", "missing ';' after declaration"); }

                        // 形状/容量校验
                        for (_, nm, nm_span, dims, ini) in &decls {
                            if let Some(init) = ini {
                                self.validate_array_initializer(nm, *nm_span, dims, init);
                            }
                        }

                        let mut stmts = vec![];
                        for (ptr, nm, _sp, dims, ini) in decls {
                            stmts.push(Stmt::VarDecl{ ty: base_ty.clone(), ptr, name: nm, array_dims: dims, init: ini });
                        }
                        items.push(Item::Global(if stmts.len()==1 { stmts.pop().unwrap() } else { Stmt::Block(stmts) }));
                    }
                } else if self.cur_is_punct(";") && Self::is_tag_type_name(&base_ty) {
                    // 顶层 tag-only 前向声明：struct Foo; / union Bar; / enum Baz;
                    self.bump(); // 吃掉 ';'，不生成 Item
                    continue;
                } else {
                    self.err_custom_here("E2101", "expected identifier after type");
                }
            } else {
                let s = self.parse_stmt();
                items.push(Item::Global(s));
            }
        }
        items
    }

    /* ===================== typedef 解析 ===================== */

    fn parse_typedef_decl(&mut self) {
        self.expect_kw("typedef");

        // 允许 typedef 前面也写 storage spec（例如很奇怪但语法上不想拒绝）
        let base_ty = match self.parse_decl_base_type() {
            Some(t) => t,
            None => {
                self.err_custom_here("E5100", "expected type name after 'typedef'");
                return;
            }
        };
        let _base_ptr = self.parse_pointer_stars(); // 当前不展开别名里的指针，只解析语法

        // typedef int MyInt, *PInt;
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
            let _dims = self.parse_array_dims_multi(); // 语法上接受数组 typedef，但不展开

            // 把别名名字记录到 typedef 符号表
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
        let _ = base_ty; // 当前不使用真实底层类型，仅记录名字
    }

    /* ===================== struct/union/enum 定义体解析 ===================== */

    fn parse_struct_body_fields(&mut self) -> Vec<StructField> {
        // 当前在 struct/union 名称之后，下一 token 为 '{'
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

            // 结构体字段：<type> declarator-list ';'
            if !self.peek_type_start() {
                self.err_custom_here("E5300", "expected field type in struct/union");
                self.sync();
                continue;
            }

            let base_ty = self.parse_decl_base_type().unwrap_or_else(|| "int".to_string());

            // 支持：
            //   int x;
            //   int x, y:3;
            //   int :3, :5;          // 无名位域
            //   struct Inner field;
            //   struct { ... } field;
            loop {
                let ptr = self.parse_pointer_stars();

                if self.cur_is(&TokenType::Identifier) {
                    // 普通具名字段
                    let name_tok = self.bump().unwrap();
                    let name = name_tok.text().to_string();
                    let dims = self.parse_array_dims_multi();

                    // 位域宽度 `: expr`
                    let bit_width = if self.cur_is_punct(":") || self.cur_is_op(":") {
                        self.bump();
                        Some(self.parse_expr())
                    } else {
                        None
                    };

                    // C 里不允许字段带初始化，这里如果出现就报错并解析掉
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
                    // 无字段名（无名位域等）
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
                    // 既不是标识符，也不是立即结束/位域的情况 -> 真正的语法错误
                    self.err_custom_here("E5301", "expected field name or ';' in struct/union");
                    break;
                }

                // 同一行多个字段：int x, y:3;
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

        loop {
            self.skip_trivia();
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
                if self.cur_is_punct(",") { self.bump(); }
                continue;
            }

            let name_tok = self.bump().unwrap();
            let cname = name_tok.text().to_string();
            let value = if self.cur_is_op("=") {
                self.bump();
                Some(self.parse_expr())
            } else {
                None
            };
            consts.push(EnumConst { name: cname, value });

            if self.cur_is_punct(",") {
                self.bump();
                if self.cur_is_punct("}") {
                    // 允许拖尾逗号
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
            // 吃掉紧跟在该 * 后面的指针修饰符（const / volatile / restrict）
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

    // [ <int>? ] ... 0..N 维
    fn parse_array_dims_multi(&mut self) -> Vec<Option<String>> {
        let mut dims = Vec::new();
        while self.cur_is_punct("[") {
            self.bump();
            let dim = if self.cur_is(&TokenType::IntConstant) {
                Some(self.bump().unwrap().text().to_string())
            } else { None };
            if !self.cur_is_punct("]") { self.err_custom_here("E3002","unterminated array dimension, expected ']'"); }
            self.expect_punct("]");
            dims.push(dim);
        }
        dims
    }

    /* ===================== 参数 ===================== */

    fn parse_params(&mut self)->Vec<Param>{
        self.expect_punct("(");
        let mut v=vec![];
        if !self.cur_is_punct(")") {
            loop {
                if !self.peek_type_start(){ self.err_custom_here("E2103", "expected type in parameter"); break; }
                let ty = self.parse_decl_base_type().unwrap_or_else(|| "int".to_string());
                let ptr=self.parse_pointer_stars();
                let name= if self.cur_is(&TokenType::Identifier){
                    self.bump().unwrap().text().to_string()
                } else { "_".into() };
                let dims = self.parse_array_dims_multi();
                v.push(Param{ ty, ptr, name, array_dims: dims });

                if self.cur_is_punct(")") { break; }
                if !self.expect_token_text(",") {
                    if !self.cur_is_punct(")") { self.err_custom_here("E2005","missing ',' between parameters"); }
                    break;
                }
            }
        }
        if !self.cur_is_punct(")") { self.err_custom_here("E3001", "unclosed parameter list, expected ')'"); }
        self.expect_punct(")");

        v
    }

    /* ===================== 语句 ===================== */

    fn parse_stmt(&mut self)->Stmt{
        self.skip_trivia();
        if self.at_end(){ return Stmt::Empty; }
        if self.cur_is_punct("{"){ return self.parse_block(); }
        if self.peek_type_start(){ return self.parse_var_decl_stmt(); }
        if self.cur_is_kw("return"){ return self.parse_return(); }
        if self.cur_is_kw("if"){ return self.parse_if(); }
        if self.cur_is_kw("while"){ return self.parse_while(); }
        if self.cur_is_kw("for"){ return self.parse_for(); }
        if self.cur_is_kw("switch"){ return self.parse_switch(); }
        if self.cur_is_kw("break"){ self.bump(); if !self.expect_token_text(";"){ self.err_custom_here("E2002","missing ';' after 'break'"); } return Stmt::Break; }
        if self.cur_is_kw("continue"){ self.bump(); if !self.expect_token_text(";"){ self.err_custom_here("E2002","missing ';' after 'continue'"); } return Stmt::Continue; }
        if self.cur_is_punct(";"){ self.bump(); return Stmt::Empty; }
        self.parse_expr_stmt()
    }

    fn parse_block(&mut self)->Stmt{
        self.expect_punct("{");
        let mut v=vec![];
        loop{
            self.skip_trivia();
            if self.at_end(){ self.err_custom_here("E3003", "unclosed block, expected '}' before EOF"); break; }
            if self.cur_is_punct("}"){ self.bump(); break; }
            v.push(self.parse_stmt());
        }
        Stmt::Block(v)
    }

    fn parse_var_decl_stmt(&mut self)->Stmt{
        let base_ty = match self.parse_decl_base_type() {
            Some(t) => t,
            None => {
                self.err_custom_here("E2100", "invalid type specifier");
                "int".to_string()
            }
        };

        let first_ptr = self.parse_pointer_stars();

        // 支持块内的 tag-only 声明：struct Foo; / union Bar; / enum Baz;
        if self.cur_is_punct(";") && Self::is_tag_type_name(&base_ty) {
            self.bump(); // 吃掉 ';'
            return Stmt::Empty;
        }

        let name_tok = if self.cur_is(&TokenType::Identifier){
            self.bump().unwrap()
        } else {
            self.err_custom_here("E2104", "expected identifier in declaration");
            self.bump().unwrap_or_else(|| self.tokens[self.i.saturating_sub(1)].clone())
        };
        let name = name_tok.text().to_string();
        let name_span = name_tok.span();

        let first_dims = self.parse_array_dims_multi();
        let init= if self.cur_is_op("=") { self.bump(); Some(self.parse_initializer()) } else { None };

        let mut decls: Vec<(usize, String, Span, Vec<Option<String>>, Option<Init>)> =
            vec![(first_ptr, name, name_span, first_dims, init)];

        while self.cur_is_punct(",") {
            self.bump();
            let ptr = self.parse_pointer_stars();
            if self.cur_is(&TokenType::Identifier){
                let nm_tok = self.bump().unwrap();
                let nm = nm_tok.text().to_string();
                let nm_span = nm_tok.span();
                let dims = self.parse_array_dims_multi();
                let ini  = if self.cur_is_op("=") { self.bump(); Some(self.parse_initializer()) } else { None };
                decls.push((ptr, nm, nm_span, dims, ini));
            } else { self.err_custom_here("E2102", "expected identifier after ',' in declaration"); break; }
        }

        if !self.expect_token_text(";") { self.err_custom_here("E2001", "missing ';' after declaration"); }

        // 校验
        for (_, nm, nm_span, dims, ini) in &decls {
            if let Some(init) = ini {
                self.validate_array_initializer(nm, *nm_span, dims, init);
            }
        }

        let mut v = vec![];
        for (ptr, nm, _sp, dims, ini) in decls {
            v.push(Stmt::VarDecl{ ty: base_ty.clone(), ptr, name: nm, array_dims: dims, init: ini });
        }
        if v.len()==1 { v.pop().unwrap() } else { Stmt::Block(v) }
    }

    fn parse_return(&mut self)->Stmt{
        self.expect_kw("return");
        if self.cur_is_punct(";"){ self.bump(); return Stmt::Return(None); }
        let e=self.parse_expr();
        if !self.expect_token_text(";") { self.err_custom_here("E2002", "missing ';' after return value"); }
        Stmt::Return(Some(e))
    }

    fn parse_if(&mut self)->Stmt{
        self.expect_kw("if");
        if !self.expect_token_text("(") { self.err_custom_here("E3004","missing '(' after 'if'"); }
        let cond=self.parse_expr();
        if !self.expect_token_text(")") { self.err_custom_here("E3001","unclosed condition, expected ')'"); }
        let then_branch=self.parse_stmt();
        let else_branch= if self.cur_is_kw("else"){ self.bump(); Some(Box::new(self.parse_stmt())) } else { None };
        Stmt::If{ cond, then_branch: Box::new(then_branch), else_branch }
    }

    fn parse_while(&mut self)->Stmt{
        self.expect_kw("while");
        if !self.expect_token_text("(") { self.err_custom_here("E3004","missing '(' after 'while'"); }
        let cond=self.parse_expr();
        if !self.expect_token_text(")") { self.err_custom_here("E3001","unclosed condition, expected ')'"); }
        let body=self.parse_stmt();
        Stmt::While{ cond, body: Box::new(body) }
    }

    fn parse_for(&mut self)->Stmt{
        self.expect_kw("for");
        if !self.expect_token_text("(") { self.err_custom_here("E3004","missing '(' after 'for'"); }

        let init = if self.cur_is_punct(";"){
            self.bump(); None
        } else if self.peek_type_start(){
            Some(Box::new(self.parse_var_decl_stmt()))
        } else {
            Some(Box::new(self.parse_expr_stmt()))
        };

        let cond = if self.cur_is_punct(";"){
            self.bump(); None
        } else {
            let e=self.parse_expr();
            if !self.expect_token_text(";") { self.err_custom_here("E2006","missing ';' in for-header after condition"); }
            Some(e)
        };

        let step = if self.cur_is_punct(")"){ None } else { Some(self.parse_expr()) };

        if !self.expect_token_text(")") { self.err_custom_here("E3001","unclosed for-header, expected ')'"); }
        let body=self.parse_stmt();
        Stmt::For{ init, cond, step, body: Box::new(body) }
    }

    fn parse_switch(&mut self)->Stmt{
        self.expect_kw("switch");
        if !self.expect_token_text("(") { self.err_custom_here("E3004","missing '(' after 'switch'"); }
        let expr = self.parse_expr();
        if !self.expect_token_text(")") { self.err_custom_here("E3001","unclosed condition, expected ')'"); }
        if !self.cur_is_punct("{") { self.err_custom_here("E3005","missing '{' to start switch-body"); }
        self.expect_punct("{");

        let mut cases: Vec<Case> = vec![];
        let mut cur_body: Vec<Stmt> = vec![];
        let mut cur_label: Option<Expr> = None;
        let mut has_label = false;

        loop {
            self.skip_trivia();
            if self.at_end(){ self.err_custom_here("E3003", "unclosed switch, expected '}' before EOF"); break; }
            if self.cur_is_punct("}") {
                if has_label {
                    cases.push(Case{ label: cur_label.take(), body: std::mem::take(&mut cur_body) });
                }
                self.bump();
                break;
            }
            if self.cur_is_kw("case") || self.cur_is_kw("default") {
                if has_label {
                    cases.push(Case{ label: cur_label.take(), body: std::mem::take(&mut cur_body) });
                    has_label = false;
                }
                if self.cur_is_kw("case") {
                    self.bump();
                    let v = self.parse_expr();
                    if !self.expect_token_text(":") { self.err_custom_here("E2007","missing ':' after case label"); }
                    cur_label = Some(v);
                    has_label = true;
                } else {
                    self.bump();
                    if !self.expect_token_text(":") { self.err_custom_here("E2007","missing ':' after default"); }
                    cur_label = None;
                    has_label = true;
                }
                continue;
            }
            cur_body.push(self.parse_stmt());
        }
        Stmt::Switch{ expr, cases }
    }

    fn parse_expr_stmt(&mut self)->Stmt{
        let e=self.parse_expr();
        if !self.expect_token_text(";") { self.err_custom_here("E2002", "missing ';' after expression"); }
        Stmt::ExprStmt(e)
    }

    /* ===================== 初始化器 ===================== */

    fn parse_initializer(&mut self) -> Init {
        if self.cur_is_punct("{") {
            self.bump();
            let mut list = Vec::new();
            if !self.cur_is_punct("}") {
                loop {
                    list.push(self.parse_initializer());
                    if self.cur_is_punct("}") { break; }
                    if self.cur_is_punct(",") {
                        self.bump();
                        if self.cur_is_punct(")") || self.cur_is_punct("}") {
                            break; // 允许拖尾逗号
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
        } else {
            Init::Expr(self.parse_assignment())
        }
    }

    /* ===================== 表达式 ===================== */

    pub fn parse_expr(&mut self)->Expr{
        let mut list = vec![ self.parse_assignment() ];
        while self.cur_is_op(",") {
            self.bump();
            list.push(self.parse_assignment());
        }
        if list.len()==1 { list.pop().unwrap() } else { Expr::Comma(list) }
    }

    fn parse_assignment(&mut self)->Expr{
        let lhs = self.parse_conditional();
        if let Some(t)=self.cur(){
            if let TokenType::Operator = t.kind() {
                let op=t.text();
                if ["=","+=","-=","*=","/=","%=","&=","|=","^="].contains(&op) {
                    let op_str=op.to_string();
                    self.bump();
                    let rhs=self.parse_assignment(); // 右结合
                    return Expr::Assign{ op: op_str, lhs: Box::new(lhs), rhs: Box::new(rhs) };
                }
            }
        }
        lhs
    }

    fn parse_conditional(&mut self)->Expr{
        let cond = self.parse_binop(0);
        if self.cur_is_op("?") {
            self.bump();
            let then_e = self.parse_assignment();
            if !self.expect_token_text(":") { self.err_custom_here("E2008","missing ':' in conditional expression"); }
            let else_e = self.parse_assignment();
            return Expr::Ternary{ cond: Box::new(cond), then_e: Box::new(then_e), else_e: Box::new(else_e) };
        }
        cond
    }

    fn precedence(op: &str)->i32{
        match op {
            "||" => 1, "&&" => 2, "|" => 3, "^" => 4, "&" => 5,
            "==" | "!=" => 6,
            "<" | ">" | "<=" | ">=" => 7,
            "<<" | ">>" => 8,
            "+" | "-" => 9,
            "*" | "/" | "%" => 10,
            _ => -1,
        }
    }

    fn parse_binop(&mut self, min_prec: i32)->Expr{
        let mut lhs=self.parse_unary();
        loop{
            let op = if let Some(t)=self.cur(){ if let TokenType::Operator = t.kind(){ t.text().to_string() } else { break; } } else { break; };
            let prec=Self::precedence(&op);
            if prec<min_prec || prec<0 { break; }
            self.bump();
            let mut rhs=self.parse_unary();
            loop {
                let next_op = if let Some(t)=self.cur(){ if let TokenType::Operator=t.kind(){ t.text().to_string() } else { break; } } else { break; };
                let next_prec=Self::precedence(&next_op);
                if next_prec>prec { rhs=self.parse_binop(next_prec); } else { break; }
            }
            lhs=Expr::Binary{ op, lhs:Box::new(lhs), rhs:Box::new(rhs) };
        }
        lhs
    }

    fn parse_unary(&mut self)->Expr{
        // sizeof / alignof —— 两种形式：对类型或对表达式
        if self.cur_is_kw("sizeof") || self.cur_is_kw("alignof") {
            let is_align = self.cur_is_kw("alignof");
            self.bump(); // eat keyword
            if self.cur_is_punct("(") {
                let mark = self.save();
                self.bump(); // '('
                if let Some(ty) = self.parse_type_name_full() {
                    if self.cur_is_punct(")") {
                        self.bump();
                        return if is_align {
                            Expr::AlignofType(ty)
                        } else {
                            Expr::SizeofType(ty)
                        };
                    }
                }
                // 回退：不是 (type) 形式，则当作 '(' expr ')' 的 sizeof/alignof
                self.restore(mark);
            }
            // 一元运算形式：sizeof unary_expr
            let e = self.parse_unary();
            return if is_align { Expr::AlignofExpr(Box::new(e)) } else { Expr::SizeofExpr(Box::new(e)) };
        }

        // 前缀 ++/--
        if self.cur_is_op("++") { self.bump(); let e=self.parse_unary(); return Expr::PreInc(Box::new(e)); }
        if self.cur_is_op("--") { self.bump(); let e=self.parse_unary(); return Expr::PreDec(Box::new(e)); }

        // cast 或 括号表达式
        if self.cur_is_punct("(") {
            let mark = self.save();
            self.bump(); // '('
            if let Some(ty) = self.parse_type_name_full() {
                if self.cur_is_punct(")") {
                    self.bump(); // ')'
                    let e = self.parse_unary();
                    return Expr::Cast { ty, expr: Box::new(e) };
                }
            }
            self.restore(mark);
        }

        // 一元 + - ! ~ & *
        if self.cur_is_op("+") || self.cur_is_op("-") || self.cur_is_op("!")
            || self.cur_is_op("~") || self.cur_is_op("&") || self.cur_is_op("*")
        {
            let op=self.bump().unwrap().text().to_string();
            let e=self.parse_unary();
            return Expr::Unary{ op, expr: Box::new(e) };
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self)->Expr{
        let mut e=self.parse_primary();
        loop{
            if self.cur_is_punct("("){
                self.bump();
                let mut args=vec![];
                if !self.cur_is_punct(")") {
                    loop {
                        args.push(self.parse_expr());
                        if self.cur_is_punct(")") { break; }
                        if !self.expect_token_text(",") { self.err_custom_here("E2009","missing ',' between call arguments"); break; }
                    }
                }
                if !self.cur_is_punct(")") { self.err_custom_here("E3001","unclosed argument list, expected ')'"); }
                self.expect_punct(")");
                if let Expr::Ident(name)=e { e=Expr::Call{ callee: name, args }; } else { self.err_custom_here("E2201","call on non-identifier"); }
                continue;
            }
            if self.cur_is_punct("["){
                self.bump();
                let idx=self.parse_expr();
                if !self.cur_is_punct("]") { self.err_custom_here("E3002","unterminated subscript, expected ']'"); }
                self.expect_punct("]");
                e = Expr::Index{ base: Box::new(e), index: Box::new(idx) };
                continue;
            }
            if self.cur_is_op("."){
                self.bump();
                if self.cur_is(&TokenType::Identifier){
                    let field = self.bump().unwrap().text().to_string();
                    e = Expr::Member{ base: Box::new(e), field };
                } else { self.err_custom_here("E2202","expected identifier after '.'"); }
                continue;
            }
            if self.cur_is_op("->"){
                self.bump();
                if self.cur_is(&TokenType::Identifier){
                    let field = self.bump().unwrap().text().to_string();
                    e = Expr::PtrMember{ base: Box::new(e), field };
                } else { self.err_custom_here("E2203","expected identifier after '->'"); }
                continue;
            }
            if self.cur_is_op("++"){ self.bump(); e = Expr::PostInc(Box::new(e)); continue; }
            if self.cur_is_op("--"){ self.bump(); e = Expr::PostDec(Box::new(e)); continue; }
            break;
        }
        e
    }

    fn parse_primary(&mut self)->Expr{
        if self.cur_is(&TokenType::IntConstant){ return Expr::Int(self.bump().unwrap().text().to_string()); }
        if self.cur_is(&TokenType::FloatConstant){ return Expr::Float(self.bump().unwrap().text().to_string()); }
        if self.cur_is(&TokenType::StringLiteral){ return Expr::Str(self.bump().unwrap().text().to_string()); }
        if self.cur_is(&TokenType::CharLiteral){ return Expr::Char(self.bump().unwrap().text().to_string()); }
        if self.cur_is(&TokenType::Identifier){ return Expr::Ident(self.bump().unwrap().text().to_string()); }
        if self.cur_is_punct("("){
            self.bump();
            let e=self.parse_expr();
            if !self.cur_is_punct(")"){ self.err_custom_here("E3001","unclosed parenthesized expression, expected ')'"); }
            self.expect_punct(")");
            return e;
        }
        self.err_custom_here("E2301", "expected expression");
        Expr::Ident("_err".into())
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
                        return None; // 非整数字面量
                    }
                }
                None => return None, // 不定长
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
                    if d > maxd { maxd = d; }
                }
                (total, maxd + 1)
            }
        }
    }

    fn validate_array_initializer(&mut self, name: &str, name_span: Span, dims: &[Option<String>], init: &Init) {
        if dims.is_empty() {
            return; // 非数组
        }
        let rank = dims.len();
        let (_cnt, depth) = Self::init_count_and_depth(init);

        if depth > rank {
            self.err_custom_span(
                "E4102",
                format!("initializer for '{}' has too many brace levels for an array of rank {}", name, rank),
                name_span,
            );
            return;
        }

        if let Some(cap) = self.dims_to_capacity(dims) {
            let (cnt, _) = Self::init_count_and_depth(init);
            if cnt > cap {
                self.err_custom_span(
                    "E4101",
                    format!("too many initializers for '{}': have {}, but capacity is {}", name, cnt, cap),
                    name_span,
                );
            }
        }
    }
}

/* ===================== 打印器（含 Init / Struct / Enum） ===================== */

pub fn stringify_items(items: &[Item]) -> String {
    fn indent(n:usize)->String{ "  ".repeat(n) }
    fn stars(n:usize)->String { "*".repeat(n) }
    fn fmt_dims(dims:&[Option<String>]) -> String {
        let mut s = String::new();
        for d in dims {
            s.push('[');
            if let Some(v)=d { s.push_str(v); }
            s.push(']');
        }
        s
    }
    fn fmt_ctype(t:&CType)->String{
        if t.ptr>0 { format!("{} {}", t.base, "*".repeat(t.ptr)) } else { t.base.clone() }
    }

    fn fmt_expr(e:&Expr, _d:usize, out:&mut String){
        match e {
            Expr::Int(v)|Expr::Float(v)|Expr::Str(v)|Expr::Char(v)|Expr::Ident(v) => out.push_str(v),
            Expr::Unary{op,expr} => { out.push('('); out.push_str(op); out.push(' '); fmt_expr(expr, 0, out); out.push(')'); }
            Expr::Binary{op,lhs,rhs} => { out.push('('); fmt_expr(lhs,0,out); out.push_str(&format!(" {} ",op)); fmt_expr(rhs,0,out); out.push(')'); }
            Expr::Call{callee,args} => { out.push_str(callee); out.push('('); for (i,a) in args.iter().enumerate(){ if i>0{out.push_str(", ");} fmt_expr(a,0,out);} out.push(')'); }
            Expr::Assign{op,lhs,rhs} => { out.push('('); fmt_expr(lhs,0,out); out.push_str(&format!(" {} ", op)); fmt_expr(rhs,0,out); out.push(')'); }
            Expr::Ternary{cond,then_e,else_e} => { out.push('('); fmt_expr(cond,0,out); out.push_str(" ? "); fmt_expr(then_e,0,out); out.push_str(" : "); fmt_expr(else_e,0,out); out.push(')'); }
            Expr::PostInc(x) => { fmt_expr(x,0,out); out.push_str("++"); }
            Expr::PostDec(x) => { fmt_expr(x,0,out); out.push_str("--"); }
            Expr::PreInc(x)  => { out.push_str("(++ "); fmt_expr(x,0,out); out.push(')'); }
            Expr::PreDec(x)  => { out.push_str("(-- "); fmt_expr(x,0,out); out.push(')'); }
            Expr::Index{base,index} => { fmt_expr(base,0,out); out.push('['); fmt_expr(index,0,out); out.push(']'); }
            Expr::Member{base,field} => { fmt_expr(base,0,out); out.push('.'); out.push_str(field); }
            Expr::PtrMember{base,field} => { fmt_expr(base,0,out); out.push_str("->"); out.push_str(field); }
            Expr::Comma(list) => {
                out.push('(');
                for (i,ee) in list.iter().enumerate(){ if i>0 { out.push_str(", "); } fmt_expr(ee,0,out); }
                out.push(')');
            }
            Expr::Cast{ty,expr} => {
                out.push_str("(("); out.push_str(&fmt_ctype(ty)); out.push_str(") ");
                fmt_expr(expr, 0, out); out.push(')');
            }
            Expr::SizeofExpr(x) => { out.push_str("sizeof "); fmt_expr(x,0,out); }
            Expr::SizeofType(t) => { out.push_str("sizeof("); out.push_str(&fmt_ctype(t)); out.push(')'); }
            Expr::AlignofExpr(x) => { out.push_str("alignof "); fmt_expr(x,0,out); }
            Expr::AlignofType(t) => { out.push_str("alignof("); out.push_str(&fmt_ctype(t)); out.push(')'); }
        }
    }

    fn fmt_init(init: &Init, d:usize, out:&mut String) {
        match init {
            Init::Expr(e) => fmt_expr(e, d, out),
            Init::List(list) => {
                out.push('{');
                for (i, it) in list.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    fmt_init(it, d, out);
                }
                out.push('}');
            }
        }
    }

    fn fmt_decl_line(
        prefix:&str,
        ty:&str,
        ptr:usize,
        name:&str,
        array_dims:&[Option<String>],
        init:&Option<Init>,
        d:usize,
        out:&mut String
    ){
        out.push_str(&format!("{}{} {}{}", indent(d), prefix, ty, if ptr>0 { format!(" {}", stars(ptr)) } else { "".into() }));
        out.push(' ');
        out.push_str(name);
        out.push_str(&fmt_dims(array_dims));
        if let Some(i)=init{
            out.push_str(" = ");
            fmt_init(i, d, out);
        }
        out.push('\n');
    }

    fn fmt_stmt(s:&Stmt, d:usize, out:&mut String){
        match s {
            Stmt::VarDecl{ty,ptr,name,array_dims,init} => {
                fmt_decl_line("decl", ty, *ptr, name, array_dims, init, d, out);
            }
            Stmt::Return(e) => {
                out.push_str(&format!("{}return", indent(d)));
                if let Some(e)=e{ out.push(' '); fmt_expr(e,d,out);}
                out.push('\n');
            }
            Stmt::If{cond,then_branch,else_branch} => {
                out.push_str(&format!("{}if ", indent(d)));
                fmt_expr(cond,d,out);
                out.push('\n');
                fmt_stmt(then_branch,d+1,out);
                if let Some(el)=else_branch{
                    out.push_str(&format!("{}else\n", indent(d)));
                    fmt_stmt(el,d+1,out);
                }
            }
            Stmt::While{cond,body} => {
                out.push_str(&format!("{}while ", indent(d)));
                fmt_expr(cond,d,out);
                out.push('\n');
                fmt_stmt(body,d+1,out);
            }
            Stmt::For{init,cond,step,body} => {
                out.push_str(&format!("{}for (", indent(d)));
                if let Some(i)=init{ fmt_stmt(i,d+1,out); } else { out.push_str("; "); }
                if let Some(c)=cond{ fmt_expr(c,d,out); }
                out.push_str("; ");
                if let Some(st)=step{ fmt_expr(st,d,out); }
                out.push_str(")\n");
                fmt_stmt(body,d+1,out);
            }
            Stmt::Switch{expr,cases} => {
                out.push_str(&format!("{}switch ", indent(d)));
                fmt_expr(expr,d,out);
                out.push_str(" {\n");
                for c in cases {
                    match &c.label {
                        Some(e) => {
                            out.push_str(&format!("{}  case ", indent(d)));
                            fmt_expr(e,d,out);
                            out.push_str(":\n");
                        }
                        None => {
                            out.push_str(&format!("{}  default:\n", indent(d)));
                        }
                    }
                    for st in &c.body { fmt_stmt(st, d+2, out); }
                }
                out.push_str(&format!("{}}}\n", indent(d)));
            }
            Stmt::Break => { out.push_str(&format!("{}break\n", indent(d))); }
            Stmt::Continue => { out.push_str(&format!("{}continue\n", indent(d))); }
            Stmt::ExprStmt(e) => {
                out.push_str(&format!("{}expr ", indent(d)));
                fmt_expr(e,d,out);
                out.push('\n');
            }
            Stmt::Block(v) => {
                out.push_str(&format!("{}block {{\n", indent(d)));
                for st in v { fmt_stmt(st, d+1, out); }
                out.push_str(&format!("{}}}\n", indent(d)));
            }
            Stmt::Empty => { out.push_str(&format!("{};\n", indent(d))); }
        }
    }

    let mut s=String::new();
    for it in items {
        match it {
            Item::Function{ret,ret_ptr,name,params,body} => {
                s.push_str(&format!(
                    "fn {}{} {}(",
                    ret,
                    if *ret_ptr>0 { format!(" {}", "*".repeat(*ret_ptr)) } else { "".into() },
                    name
                ));
                for (i,p) in params.iter().enumerate(){
                    if i>0{s.push_str(", "); }
                    let dims = {
                        let mut tmp = String::new();
                        for d in &p.array_dims {
                            tmp.push('[');
                            if let Some(v) = d { tmp.push_str(v); }
                            tmp.push(']');
                        }
                        tmp
                    };
                    s.push_str(&format!(
                        "{}{} {}{}",
                        p.ty,
                        if p.ptr>0 { format!(" {}", "*".repeat(p.ptr)) } else { "".into() },
                        p.name,
                        dims
                    ));
                }
                s.push_str(")\n");
                fmt_stmt(body,1,&mut s);
            }
            Item::Global(g) => {
                s.push_str("global ");
                fmt_stmt(g,0,&mut s);
            }
            Item::StructDef{kind,name,fields} => {
                let kind_str = match kind {
                    StructKind::Struct => "struct",
                    StructKind::Union  => "union",
                };
                s.push_str(&format!("{} {} {{\n", kind_str, name));
                for f in fields {
                    s.push_str("  field ");
                    s.push_str(&f.ty);
                    if f.ptr>0 {
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
            Item::EnumDef{name,consts} => {
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
