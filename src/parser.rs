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
    // 新增
    PostInc(Box<Expr>),
    PostDec(Box<Expr>),
    Index  { base: Box<Expr>, index: Box<Expr> },
    Member { base: Box<Expr>, field: String },     // a.b
    PtrMember { base: Box<Expr>, field: String },  // a->b
    Comma(Vec<Expr>), // e1, e2, e3 ；取最后一个值
}

#[derive(Debug, Clone)]
pub struct Param { pub ty: String, pub name: String }

#[derive(Debug, Clone)]
pub enum Stmt {
    VarDecl { ty: String, name: String, init: Option<Expr> },
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
    Function { ret: String, name: String, params: Vec<Param>, body: Stmt },
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
                // 可能是函数或全局变量声明
                let ty = self.bump().unwrap().text().to_string();
                if self.cur_is(&TokenType::Identifier) {
                    let name_tok = self.bump().unwrap();
                    let name = name_tok.text().to_string();
                    if self.cur_is_punct("(") {
                        let params = self.parse_params();
                        let body = if self.cur_is_punct("{") {
                            self.parse_block()
                        } else { self.error("function must have a body".into()); Stmt::Empty };
                        items.push(Item::Function{ ret: ty, name, params, body });
                    } else {
                        // 多声明 int a=1,b=2;
                        let mut decls: Vec<(String, Option<Expr>)> = vec![];
                        let first_init = if self.cur_is_op("=") { self.bump(); Some(self.parse_expr()) } else { None };
                        decls.push((name, first_init));
                        while self.cur_is_punct(",") {
                            self.bump();
                            if self.cur_is(&TokenType::Identifier) {
                                let nm = self.bump().unwrap().text().to_string();
                                let init = if self.cur_is_op("=") { self.bump(); Some(self.parse_expr()) } else { None };
                                decls.push((nm, init));
                            } else { self.error("expected identifier after ',' in declaration".into()); break; }
                        }
                        self.expect_punct(";");
                        let mut stmts = vec![];
                        for (nm, init) in decls {
                            stmts.push(Stmt::VarDecl{ ty: ty.clone(), name: nm, init });
                        }
                        items.push(Item::Global(if stmts.len()==1 { stmts.pop().unwrap() } else { Stmt::Block(stmts) }));
                    }
                } else { self.error("expected identifier after type".into()); }
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

    fn parse_params(&mut self)->Vec<Param>{
        self.expect_punct("(");
        let mut v=vec![];
        if !self.cur_is_punct(")") {
            loop {
                if !self.peek_type_keyword(){ self.error("expected type in parameter".into()); break; }
                let ty=self.bump().unwrap().text().to_string();
                let name= if self.cur_is(&TokenType::Identifier){ self.bump().unwrap().text().to_string() } else { self.error("expected param name".into()); "_".into() };
                v.push(Param{ ty, name });
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
        let ty=self.bump().unwrap().text().to_string();
        let mut decls: Vec<(String, Option<Expr>)> = vec![];
        let name= if self.cur_is(&TokenType::Identifier){ self.bump().unwrap().text().to_string() } else { self.error("expected identifier".into()); "_err".into() };
        let init= if self.cur_is_op("=") { self.bump(); Some(self.parse_expr()) } else { None };
        decls.push((name, init));
        while self.cur_is_punct(",") {
            self.bump();
            if self.cur_is(&TokenType::Identifier){
                let nm = self.bump().unwrap().text().to_string();
                let ini = if self.cur_is_op("=") { self.bump(); Some(self.parse_expr()) } else { None };
                decls.push((nm, ini));
            } else { self.error("expected identifier after ',' in declaration".into()); break; }
        }
        self.expect_punct(";");
        let mut v = vec![];
        for (nm, ini) in decls { v.push(Stmt::VarDecl{ ty: ty.clone(), name: nm, init: ini }); }
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

    /* ---------- 表达式 ---------- */

    pub fn parse_expr(&mut self)->Expr{
        // 逗号最低优先级，左结合： e1, e2, e3
        let mut list = vec![ self.parse_assignment() ];
        while self.cur_is_op(",") {
            self.bump();
            list.push(self.parse_assignment());
        }
        if list.len()==1 { list.pop().unwrap() } else { Expr::Comma(list) }
    }

    // 赋值（右结合），下层是条件表达式
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

    // 条件 ?: 右结合；内部跟随 assignment 级别
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

    // 二元优先级（越大优先级越高）
    // 参考 C 但做了简化：||(1) &&(2) |(3) ^(4) &(5) == !=(6) < > <= >=(7) << >>(8) +(9) -(9) *(10)/(10)%(10)
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
        // 一元 + - !
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
            // 调用
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
                if let Expr::Ident(name)=e {
                    e=Expr::Call{ callee: name, args };
                } else {
                    // 允许对任意表达式调用（例如 (fptr)()），这里简单容忍：把 callee 打平到字符串
                    // 但为了保持简洁，遇到这种情况给个错误提示
                    self.error("call on non-identifier".into());
                }
                continue;
            }
            // 下标
            if self.cur_is_punct("["){
                self.bump();
                let idx=self.parse_expr();
                self.expect_punct("]");
                e = Expr::Index{ base: Box::new(e), index: Box::new(idx) };
                continue;
            }
            // 成员 .
            if self.cur_is_op("."){
                self.bump();
                if self.cur_is(&TokenType::Identifier){
                    let field = self.bump().unwrap().text().to_string();
                    e = Expr::Member{ base: Box::new(e), field };
                } else { self.error("expected identifier after '.'".into()); }
                continue;
            }
            // 指针成员 ->
            if self.cur_is_op("->"){
                self.bump();
                if self.cur_is(&TokenType::Identifier){
                    let field = self.bump().unwrap().text().to_string();
                    e = Expr::PtrMember{ base: Box::new(e), field };
                } else { self.error("expected identifier after '->'".into()); }
                continue;
            }
            // 后缀 ++ / --
            if self.cur_is_op("++"){
                self.bump();
                e = Expr::PostInc(Box::new(e));
                continue;
            }
            if self.cur_is_op("--"){
                self.bump();
                e = Expr::PostDec(Box::new(e));
                continue;
            }
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
    fn fmt_stmt(s:&Stmt, d:usize, out:&mut String){
        match s {
            Stmt::VarDecl{ty,name,init} => {
                out.push_str(&format!("{}decl {} {}", indent(d), ty, name));
                if let Some(e)=init{ out.push_str(" = "); fmt_expr(e,d,out); }
                out.push('\n');
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
            Item::Function{ret,name,params,body} => {
                s.push_str(&format!("fn {} {}(", ret, name));
                for (i,p) in params.iter().enumerate(){ if i>0{s.push_str(", ");} s.push_str(&format!("{} {}", p.ty, p.name)); }
                s.push_str(")\n"); fmt_stmt(body,1,&mut s);
            }
            Item::Global(g) => { s.push_str("global "); fmt_stmt(g,0,&mut s); }
        }
    }
    s
}
