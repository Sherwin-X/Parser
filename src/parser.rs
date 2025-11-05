use crate::token::{Token, TokenType};

#[derive(Debug, Clone)]
pub enum Expr {
    Int(String),
    Float(String),
    Str(String),
    Char(String),
    Ident(String),
    Binary { op: String, lhs: Box<Expr>, rhs: Box<Expr> },
    Unary  { op: String, expr: Box<Expr> },
    Call   { callee: String, args: Vec<Expr> },
    Assign { op: String, lhs: Box<Expr>, rhs: Box<Expr> },
    Ternary { cond: Box<Expr>, then_e: Box<Expr>, else_e: Box<Expr> },
    PostInc(Box<Expr>),
    PostDec(Box<Expr>),
    Index  { base: Box<Expr>, index: Box<Expr> },
    Member { base: Box<Expr>, field: String },
    PtrMember { base: Box<Expr>, field: String },
    Comma(Vec<Expr>),
}

/* ============ C 风格声明增强：指针 + 多维数组 ============ */

#[derive(Debug, Clone)]
pub struct Param {
    pub ty: String,          // 基本类型：int/char/float/double/void
    pub ptr: usize,          // '*' 的数量
    pub name: String,        // 形参名（允许 "_" 占位）
    pub array_dims: Vec<Option<String>>, // 多维数组，如 [3][4] 或 [][]（省略用 None）
}

#[derive(Debug, Clone)]
pub enum Stmt {
    // 支持指针与多维数组
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
    pub label: Option<Expr>, // None 表示 default
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Item {
    // 返回类型仍支持指针层数
    Function { ret: String, ret_ptr: usize, name: String, params: Vec<Param>, body: Stmt },
    Global(Stmt),
}

#[derive(Debug, Clone)]
pub struct ParseError { pub message: String, pub at: usize }

pub struct Parser { tokens: Vec<Token>, i: usize, pub errors: Vec<ParseError> }

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self { Self { tokens, i:0, errors: vec![] } }

    fn at_end(&self)->bool{ self.i>=self.tokens.len() }
    fn cur(&self)->Option<&Token>{ self.tokens.get(self.i) }
    fn cur_is(&self, k: &TokenType)->bool{ self.cur().map(|t| t.kind()==k).unwrap_or(false) }
    fn cur_is_kw(&self, kw: &str)->bool{ self.cur().map(|t| matches!(t.kind(), TokenType::Keyword) && t.text()==kw).unwrap_or(false) }
    fn cur_is_punct(&self, ch: &str)->bool{ self.cur().map(|t| matches!(t.kind(), TokenType::Punctuation) && t.text()==ch).unwrap_or(false) }
    fn cur_is_op(&self, op: &str)->bool{ self.cur().map(|t| matches!(t.kind(), TokenType::Operator) && t.text()==op).unwrap_or(false) }
    fn bump(&mut self)->Option<Token>{ if self.at_end(){None}else{ let t=self.tokens[self.i].clone(); self.i+=1; Some(t) } }
    fn expect_punct(&mut self, ch: &str){ if !self.cur_is_punct(ch){ self.error(format!("expected '{}'", ch)); } else { self.bump(); } }
    fn expect_kw(&mut self, kw: &str){ if !self.cur_is_kw(kw){ self.error(format!("expected keyword '{}'", kw)); } else { self.bump(); } }
    fn error(&mut self, msg: String){ self.errors.push(ParseError{ message: msg, at: self.i }); self.sync(); }
    fn sync(&mut self){ while !self.at_end() { if self.cur_is_punct(";") || self.cur_is_punct("}") { return; } self.i += 1; } }

    /* ---------- 顶层 ---------- */

    pub fn parse_items(&mut self) -> Vec<Item> {
        let mut items = vec![];
        while !self.at_end() {
            self.skip_trivia();
            if self.at_end(){ break; }
            if self.peek_type_keyword() {
                let base_ty = self.bump().unwrap().text().to_string();
                let ret_ptr = self.parse_pointer_stars();

                if self.cur_is(&TokenType::Identifier) {
                    let name_tok = self.bump().unwrap();
                    let name = name_tok.text().to_string();

                    if self.cur_is_punct("(") {
                        let params = self.parse_params();
                        let body = if self.cur_is_punct("{") {
                            self.parse_block()
                        } else { self.error("function must have a body".into()); Stmt::Empty };
                        items.push(Item::Function{ ret: base_ty, ret_ptr, name, params, body });
                    } else {
                        // 全局变量声明：首个声明符（已取到 name）
                        let first_dims = self.parse_array_dims_multi();
                        let first_init = if self.cur_is_op("=") { self.bump(); Some(self.parse_expr()) } else { None };

                        let mut decls: Vec<(usize, String, Vec<Option<String>>, Option<Expr>)> =
                            vec![(0, name, first_dims, first_init)];

                        // 继续解析 , 声明符
                        while self.cur_is_punct(",") {
                            self.bump();
                            let ptr = self.parse_pointer_stars();
                            if self.cur_is(&TokenType::Identifier) {
                                let nm = self.bump().unwrap().text().to_string();
                                let dims = self.parse_array_dims_multi();
                                let ini  = if self.cur_is_op("=") { self.bump(); Some(self.parse_expr()) } else { None };
                                decls.push((ptr, nm, dims, ini));
                            } else { self.error("expected identifier after ',' in declaration".into()); break; }
                        }
                        self.expect_punct(";");

                        let mut stmts = vec![];
                        for (ptr, nm, dims, ini) in decls {
                            stmts.push(Stmt::VarDecl{ ty: base_ty.clone(), ptr, name: nm, array_dims: dims, init: ini });
                        }
                        items.push(Item::Global(if stmts.len()==1 { stmts.pop().unwrap() } else { Stmt::Block(stmts) }));
                    }
                } else {
                    self.error("expected identifier after type".into());
                }
            } else {
                let s = self.parse_stmt();
                items.push(Item::Global(s));
            }
        }
        items
    }

    fn skip_trivia(&mut self){
        while let Some(t)=self.cur() {
            if matches!(t.kind(), TokenType::Whitespace | TokenType::Comment | TokenType::Preprocessor) { self.i+=1; } else { break; }
        }
    }

    /* ---------- 参数与声明子 ---------- */

    fn parse_pointer_stars(&mut self) -> usize {
        let mut n=0usize;
        while self.cur_is_op("*") { self.bump(); n+=1; }
        n
    }

    // 解析 0..N 个维度： [ <int-const>? ] ...
    fn parse_array_dims_multi(&mut self) -> Vec<Option<String>> {
        let mut dims = Vec::new();
        while self.cur_is_punct("[") {
            self.bump(); // '['
            let dim = if self.cur_is(&TokenType::IntConstant) {
                Some(self.bump().unwrap().text().to_string())
            } else {
                // 允许省略尺寸：[]
                None
            };
            self.expect_punct("]");
            dims.push(dim);
        }
        dims
    }

    fn parse_params(&mut self)->Vec<Param>{
        self.expect_punct("(");
        let mut v=vec![];
        if !self.cur_is_punct(")") {
            loop {
                if !self.peek_type_keyword(){ self.error("expected type in parameter".into()); break; }
                let ty=self.bump().unwrap().text().to_string();
                let ptr=self.parse_pointer_stars();
                let name= if self.cur_is(&TokenType::Identifier){
                    self.bump().unwrap().text().to_string()
                } else { "_".into() };
                let dims = self.parse_array_dims_multi(); // 形参也可有维度（会按 C 规则退化为指针，但这里仅保留语法）
                v.push(Param{ ty, ptr, name, array_dims: dims });
                if self.cur_is_punct(")"){ break; }
                self.expect_punct(",");
            }
        }
        self.expect_punct(")");
        v
    }

    /* ---------- 语句 ---------- */

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
        if self.cur_is_kw("break"){ self.bump(); self.expect_punct(";"); return Stmt::Break; }
        if self.cur_is_kw("continue"){ self.bump(); self.expect_punct(";"); return Stmt::Continue; }
        if self.cur_is_punct(";"){ self.bump(); return Stmt::Empty; }
        self.parse_expr_stmt()
    }

    fn parse_block(&mut self)->Stmt{
        self.expect_punct("{");
        let mut v=vec![];
        loop{
            self.skip_trivia();
            if self.at_end(){ break; }
            if self.cur_is_punct("}"){ self.bump(); break; }
            v.push(self.parse_stmt());
        }
        Stmt::Block(v)
    }

    fn peek_type_keyword(&self)->bool{
        self.cur_is_kw("int") || self.cur_is_kw("float") || self.cur_is_kw("char")
            || self.cur_is_kw("double") || self.cur_is_kw("void")
    }

    fn parse_var_decl_stmt(&mut self)->Stmt{
        let base_ty=self.bump().unwrap().text().to_string();

        // 第一项：* name [dims...] (= init)?
        let first_ptr = self.parse_pointer_stars();
        let name= if self.cur_is(&TokenType::Identifier){
            self.bump().unwrap().text().to_string()
        } else { self.error("expected identifier".into()); "_err".into() };
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
            } else { self.error("expected identifier after ',' in declaration".into()); break; }
        }
        self.expect_punct(";");

        let mut v = vec![];
        for (ptr, nm, dims, ini) in decls {
            v.push(Stmt::VarDecl{ ty: base_ty.clone(), ptr, name: nm, array_dims: dims, init: ini });
        }
        if v.len()==1 { v.pop().unwrap() } else { Stmt::Block(v) }
    }

    fn parse_return(&mut self)->Stmt{
        self.expect_kw("return");
        if self.cur_is_punct(";"){ self.bump(); return Stmt::Return(None); }
        let e=self.parse_expr(); self.expect_punct(";"); Stmt::Return(Some(e))
    }

    fn parse_if(&mut self)->Stmt{
        self.expect_kw("if"); self.expect_punct("(");
        let cond=self.parse_expr();
        self.expect_punct(")");
        let then_branch=self.parse_stmt();
        let else_branch= if self.cur_is_kw("else"){ self.bump(); Some(Box::new(self.parse_stmt())) } else { None };
        Stmt::If{ cond, then_branch: Box::new(then_branch), else_branch }
    }

    fn parse_while(&mut self)->Stmt{
        self.expect_kw("while"); self.expect_punct("(");
        let cond=self.parse_expr();
        self.expect_punct(")");
        let body=self.parse_stmt();
        Stmt::While{ cond, body: Box::new(body) }
    }

    fn parse_for(&mut self)->Stmt{
        self.expect_kw("for"); self.expect_punct("(");
        let init = if self.cur_is_punct(";"){ self.bump(); None }
                   else if self.peek_type_keyword(){ Some(Box::new(self.parse_var_decl_stmt())) }
                   else { Some(Box::new(self.parse_expr_stmt())) };
        let cond = if self.cur_is_punct(";"){ self.bump(); None } else { let e=self.parse_expr(); self.expect_punct(";"); Some(e) };
        let step = if self.cur_is_punct(")"){ None } else { Some(self.parse_expr()) };
        self.expect_punct(")");
        let body=self.parse_stmt();
        Stmt::For{ init, cond, step, body: Box::new(body) }
    }

    fn parse_switch(&mut self)->Stmt{
        self.expect_kw("switch");
        self.expect_punct("(");
        let expr = self.parse_expr();
        self.expect_punct(")");
        self.expect_punct("{");

        let mut cases: Vec<Case> = vec![];
        let mut cur_body: Vec<Stmt> = vec![];
               let mut cur_label: Option<Expr> = None;
        let mut has_label = false;

        loop {
            self.skip_trivia();
            if self.at_end(){ break; }
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
                    self.expect_punct(":");
                    cur_label = Some(v);
                    has_label = true;
                } else {
                    self.bump();
                    self.expect_punct(":");
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
        let e=self.parse_expr(); self.expect_punct(";"); Stmt::ExprStmt(e)
    }

    /* ---------- 表达式（沿用你当前增强） ---------- */

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
                    let rhs=self.parse_assignment();
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
            if !self.cur_is_op(":") { self.error("expected ':' in conditional expression".into()); return cond; }
            self.bump();
            let else_e = self.parse_assignment();
            return Expr::Ternary{ cond: Box::new(cond), then_e: Box::new(then_e), else_e: Box::new(else_e) };
        }
        cond
    }

    fn precedence(op: &str)->i32{
        match op {
            "||" => 1,
            "&&" => 2,
            "|"  => 3,
            "^"  => 4,
            "&"  => 5,
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
        if self.cur_is_op("+") || self.cur_is_op("-") || self.cur_is_op("!") {
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
                        self.expect_punct(",");
                    }
                }
                self.expect_punct(")");
                if let Expr::Ident(name)=e { e=Expr::Call{ callee: name, args }; } else { self.error("call on non-identifier".into()); }
                continue;
            }
            if self.cur_is_punct("["){
                self.bump();
                let idx=self.parse_expr();
                self.expect_punct("]");
                e = Expr::Index{ base: Box::new(e), index: Box::new(idx) };
                continue;
            }
            if self.cur_is_op("."){
                self.bump();
                if self.cur_is(&TokenType::Identifier){
                    let field = self.bump().unwrap().text().to_string();
                    e = Expr::Member{ base: Box::new(e), field };
                } else { self.error("expected identifier after '.'".into()); }
                continue;
            }
            if self.cur_is_op("->"){
                self.bump();
                if self.cur_is(&TokenType::Identifier){
                    let field = self.bump().unwrap().text().to_string();
                    e = Expr::PtrMember{ base: Box::new(e), field };
                } else { self.error("expected identifier after '->'".into()); }
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
        if self.cur_is_punct("("){ self.bump(); let e=self.parse_expr(); self.expect_punct(")"); return e; }
        self.error("expected expression".into()); Expr::Ident("_err".into())
    }
}

/* ---------- 打印 ---------- */

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
