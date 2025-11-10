// parser.rs
use crate::token::{Token, TokenType, Span};

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
}

#[derive(Debug, Clone)]
pub struct Param {
    pub ty: String,
    pub ptr: usize,
    pub name: String,                         // 允许 "_" 占位
    pub array_dims: Vec<Option<String>>,      // 多维数组维度列表
}

#[derive(Debug, Clone)]
pub enum Stmt {
    VarDecl { ty: String, ptr: usize, name: String, array_dims: Vec<Option<String>>, init: Option<Expr> },
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

#[derive(Debug, Clone)]
pub enum Item {
    Function { ret: String, ret_ptr: usize, name: String, params: Vec<Param>, body: Stmt },
    Global(Stmt),
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
    source: String,         // 保存整段源码用于取行文本
    pub errors: Vec<ParseError>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, source: String) -> Self {
        Self { tokens, i: 0, source, errors: vec![] }
    }

    #[inline] fn at_end(&self)->bool{ self.i>=self.tokens.len() }
    #[inline] fn cur(&self)->Option<&Token>{ self.tokens.get(self.i) }
    #[inline] fn cur_is(&self, k: &TokenType)->bool{ self.cur().map(|t| t.kind()==k).unwrap_or(false) }
    #[inline] fn cur_is_kw(&self, kw: &str)->bool{ self.cur().map(|t| matches!(t.kind(), TokenType::Keyword) && t.text()==kw).unwrap_or(false) }
    #[inline] fn cur_is_punct(&self, ch: &str)->bool{ self.cur().map(|t| matches!(t.kind(), TokenType::Punctuation) && t.text()==ch).unwrap_or(false) }
    #[inline] fn cur_is_op(&self, op: &str)->bool{ self.cur().map(|t| matches!(t.kind(), TokenType::Operator) && t.text()==op).unwrap_or(false) }
    fn bump(&mut self)->Option<Token>{ if self.at_end(){None}else{ let t=self.tokens[self.i].clone(); self.i+=1; Some(t) } }

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

    /* ===================== 入口 ===================== */

    pub fn parse_items(&mut self) -> Vec<Item> {
        let mut items = vec![];
        while !self.at_end() {
            self.skip_trivia();
            if self.at_end(){ break; }

            if self.peek_type_keyword() {
                let base_ty = self.bump().unwrap().text().to_string();
                let ret_ptr = self.parse_pointer_stars();

                if self.cur_is(&TokenType::Identifier) {
                    let name = self.bump().unwrap().text().to_string();

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
                        let first_init = if self.cur_is_op("=") { self.bump(); Some(self.parse_expr()) } else { None };
                        let mut decls: Vec<(usize, String, Vec<Option<String>>, Option<Expr>)> =
                            vec![(0, name, first_dims, first_init)];

                        while self.cur_is_punct(",") {
                            self.bump();
                            let ptr = self.parse_pointer_stars();
                            if self.cur_is(&TokenType::Identifier) {
                                let nm = self.bump().unwrap().text().to_string();
                                let dims = self.parse_array_dims_multi();
                                let ini  = if self.cur_is_op("=") { self.bump(); Some(self.parse_expr()) } else { None };
                                decls.push((ptr, nm, dims, ini));
                            } else { self.err_custom_here("E2102", "expected identifier after ',' in declaration"); break; }
                        }
                        if !self.expect_token_text(";"){ self.err_custom_here("E2001", "missing ';' after declaration"); }

                        let mut stmts = vec![];
                        for (ptr, nm, dims, ini) in decls {
                            stmts.push(Stmt::VarDecl{ ty: base_ty.clone(), ptr, name: nm, array_dims: dims, init: ini });
                        }
                        items.push(Item::Global(if stmts.len()==1 { stmts.pop().unwrap() } else { Stmt::Block(stmts) }));
                    }
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

    /* ===================== 辅助 ===================== */

    fn peek_type_keyword(&self)->bool{
        self.cur_is_kw("int") || self.cur_is_kw("float") || self.cur_is_kw("char")
            || self.cur_is_kw("double") || self.cur_is_kw("void")
    }

    fn parse_pointer_stars(&mut self) -> usize {
        let mut n=0usize;
        while self.cur_is_op("*") { self.bump(); n+=1; }
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
                if !self.peek_type_keyword(){ self.err_custom_here("E2103", "expected type in parameter"); break; }
                let ty=self.bump().unwrap().text().to_string();
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
        if self.peek_type_keyword(){ return self.parse_var_decl_stmt(); }
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
        let base_ty=self.bump().unwrap().text().to_string();

        let first_ptr = self.parse_pointer_stars();
        let name= if self.cur_is(&TokenType::Identifier){
            self.bump().unwrap().text().to_string()
        } else { self.err_custom_here("E2104", "expected identifier in declaration"); "_err".into() };
        let first_dims = self.parse_array_dims_multi();
        let init= if self.cur_is_op("=") { self.bump(); Some(self.parse_expr()) } else { None };

        let mut decls: Vec<(usize, String, Vec<Option<String>>, Option<Expr>)> =
            vec![(first_ptr, name, first_dims, init)];

        while self.cur_is_punct(",") {
            self.bump();
            let ptr = self.parse_pointer_stars();
            if self.cur_is(&TokenType::Identifier){
                let nm = self.bump().unwrap().text().to_string();
                let dims = self.parse_array_dims_multi();
                let ini  = if self.cur_is_op("=") { self.bump(); Some(self.parse_expr()) } else { None };
                decls.push((ptr, nm, dims, ini));
            } else { self.err_custom_here("E2102", "expected identifier after ',' in declaration"); break; }
        }

        if !self.expect_token_text(";") { self.err_custom_here("E2001", "missing ';' after declaration"); }

        let mut v = vec![];
        for (ptr, nm, dims, ini) in decls {
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
        } else if self.peek_type_keyword(){
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
        if self.cur_is_op("++") { self.bump(); let e=self.parse_unary(); return Expr::PreInc(Box::new(e)); }
        if self.cur_is_op("--") { self.bump(); let e=self.parse_unary(); return Expr::PreDec(Box::new(e)); }
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
        if self.cur_is_punct("("){ self.bump(); let e=self.parse_expr(); if !self.cur_is_punct(")"){ self.err_custom_here("E3001","unclosed parenthesized expression, expected ')'"); } self.expect_punct(")"); return e; }
        self.err_custom_here("E2301", "expected expression");
        Expr::Ident("_err".into())
    }
}

/* ===================== 打印器（保持不变，方便调试） ===================== */

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
        }
    }

    fn fmt_decl_line(prefix:&str, ty:&str, ptr:usize, name:&str, array_dims:&[Option<String>], init:&Option<Expr>, d:usize, out:&mut String){
        out.push_str(&format!("{}{} {}{}", indent(d), prefix, ty, if ptr>0 { format!(" {}", stars(ptr)) } else { "".into() }));
        out.push(' ');
        out.push_str(name);
        out.push_str(&fmt_dims(array_dims));
        if let Some(e)=init{ out.push_str(" = "); fmt_expr(e,d,out); }
        out.push('\n');
    }

    fn fmt_stmt(s:&Stmt, d:usize, out:&mut String){
        match s {
            Stmt::VarDecl{ty,ptr,name,array_dims,init} => {
                fmt_decl_line("decl", ty, *ptr, name, array_dims, init, d, out);
            }
            Stmt::Return(e) => { out.push_str(&format!("{}return", indent(d))); if let Some(e)=e{ out.push(' '); fmt_expr(e,d,out);} out.push('\n'); }
            Stmt::If{cond,then_branch,else_branch} => {
                out.push_str(&format!("{}if ", indent(d))); fmt_expr(cond,d,out); out.push('\n');
                fmt_stmt(then_branch,d+1,out);
                if let Some(el)=else_branch{ out.push_str(&format!("{}else\n", indent(d))); fmt_stmt(el,d+1,out); }
            }
            Stmt::While{cond,body} => { out.push_str(&format!("{}while ", indent(d))); fmt_expr(cond,d,out); out.push('\n'); fmt_stmt(body,d+1,out); }
            Stmt::For{init,cond,step,body} => {
                out.push_str(&format!("{}for (", indent(d)));
                if let Some(i)=init{ fmt_stmt(i,d+1,out); } else { out.push_str("; "); }
                if let Some(c)=cond{ fmt_expr(c,d,out); } out.push_str("; ");
                if let Some(st)=step{ fmt_expr(st,d,out); } out.push_str(")\n");
                fmt_stmt(body,d+1,out);
            }
            Stmt::Switch{expr,cases} => {
                out.push_str(&format!("{}switch ", indent(d))); fmt_expr(expr,d,out); out.push_str(" {\n");
                for c in cases {
                    match &c.label {
                        Some(e) => { out.push_str(&format!("{}  case ", indent(d))); fmt_expr(e,d,out); out.push_str(":\n"); }
                        None => { out.push_str(&format!("{}  default:\n", indent(d))); }
                    }
                    for st in &c.body { fmt_stmt(st, d+2, out); }
                }
                out.push_str(&format!("{}}}\n", indent(d)));
            }
            Stmt::Break => { out.push_str(&format!("{}break\n", indent(d))); }
            Stmt::Continue => { out.push_str(&format!("{}continue\n", indent(d))); }
            Stmt::ExprStmt(e) => { out.push_str(&format!("{}expr ", indent(d))); fmt_expr(e,d,out); out.push('\n'); }
            Stmt::Block(v) => { out.push_str(&format!("{}block {{\n", indent(d))); for st in v { fmt_stmt(st, d+1, out); } out.push_str(&format!("{}}}\n", indent(d))); }
            Stmt::Empty => { out.push_str(&format!("{};\n", indent(d))); }
        }
    }

    let mut s=String::new();
    for it in items {
        match it {
            Item::Function{ret,ret_ptr,name,params,body} => {
                s.push_str(&format!("fn {}{} {}(", ret, if *ret_ptr>0 { format!(" {}", "*".repeat(*ret_ptr)) } else { "".into() }, name));
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
                    s.push_str(&format!("{}{} {}{}", p.ty, if p.ptr>0 { format!(" {}", "*".repeat(p.ptr)) } else { "".into() }, p.name, dims));
                }
                s.push_str(")\n"); fmt_stmt(body,1,&mut s);
            }
            Item::Global(g) => { s.push_str("global "); fmt_stmt(g,0,&mut s); }
        }
    }
    s
}
