
use crate::token::{Token, TokenType};

#[derive(Debug, Clone)]
pub enum Expr {
    Int(String), Float(String), Str(String), Char(String), Ident(String),
    Binary{op:String, lhs: Box<Expr>, rhs: Box<Expr>},
    Unary{op:String, expr: Box<Expr>},
    Call{ callee: String, args: Vec<Expr> },
}

#[derive(Debug, Clone)]
pub enum Stmt {
    VarDecl{ ty:String, name:String, init: Option<Expr> },
    Return(Option<Expr>),
    ExprStmt(Expr),
    Block(Vec<Stmt>),
    Empty,
}

#[derive(Debug, Clone)]
pub struct ParseError { pub message: String, pub at: usize }

pub struct Parser { tokens: Vec<Token>, i: usize, pub errors: Vec<ParseError> }
impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self { Self { tokens, i:0, errors: vec![] } }
    fn at_end(&self)->bool{ self.i>=self.tokens.len() }
    fn cur(&self)->Option<&Token>{ self.tokens.get(self.i) }
    fn cur_text(&self)->Option<&str>{ self.cur().map(|t| t.text()) }
    fn cur_is(&self, k: &TokenType)->bool{ self.cur().map(|t| t.kind()==k).unwrap_or(false) }
    fn cur_is_kw(&self, kw: &str)->bool{ self.cur().map(|t| matches!(t.kind(), TokenType::Keyword) && t.text()==kw).unwrap_or(false) }
    fn cur_is_punct(&self, ch: &str)->bool{ self.cur().map(|t| matches!(t.kind(), TokenType::Punctuation) && t.text()==ch).unwrap_or(false) }
    fn cur_is_op(&self, op: &str)->bool{ self.cur().map(|t| matches!(t.kind(), TokenType::Operator) && t.text()==op).unwrap_or(false) }
    fn bump(&mut self)->Option<Token>{ if self.at_end(){None}else{ let t=self.tokens[self.i].clone(); self.i+=1; Some(t) } }
    fn expect_punct(&mut self, ch: &str){ if !self.cur_is_punct(ch){ self.error(format!("expected '{}'", ch)); } else { self.bump(); } }
    fn expect_kw(&mut self, kw: &str){ if !self.cur_is_kw(kw){ self.error(format!("expected keyword '{}'", kw)); } else { self.bump(); } }
    fn error(&mut self, msg: String){ self.errors.push(ParseError{ message: msg, at: self.i }); self.sync(); }
    fn sync(&mut self){ while !self.at_end() { if self.cur_is_punct(";") || self.cur_is_punct("}") { return; } self.i += 1; } }

    pub fn parse_program(&mut self) -> Vec<Stmt> {
        let mut stmts = vec![];
        while !self.at_end() {
            while let Some(t)=self.cur() {
                if matches!(t.kind(), TokenType::Whitespace | TokenType::Comment | TokenType::Preprocessor) { self.i+=1; } else { break; }
            }
            if self.at_end(){ break; }
            if self.cur_is_punct("}") { self.bump(); continue; }
            if self.peek_type_keyword() { stmts.push(self.parse_var_decl()); }
            else if self.cur_is_kw("return") { stmts.push(self.parse_return()); }
            else if self.cur_is_punct("{") { stmts.push(self.parse_block()); }
            else if self.cur_is_punct(";") { self.bump(); stmts.push(Stmt::Empty); }
            else { stmts.push(self.parse_expr_stmt()); }
        }
        stmts
    }
    fn peek_type_keyword(&self)->bool{ self.cur_is_kw("int") || self.cur_is_kw("float") || self.cur_is_kw("char") || self.cur_is_kw("double") }
    fn parse_block(&mut self)->Stmt{
        self.expect_punct("{"); let mut v=vec![];
        loop{
            while let Some(t)=self.cur(){ if matches!(t.kind(), TokenType::Whitespace | TokenType::Comment) { self.i+=1; } else { break; } }
            if self.at_end(){ break; }
            if self.cur_is_punct("}") { self.bump(); break; }
            if self.peek_type_keyword(){ v.push(self.parse_var_decl()); }
            else if self.cur_is_kw("return"){ v.push(self.parse_return()); }
            else if self.cur_is_punct("{"){ v.push(self.parse_block()); }
            else if self.cur_is_punct(";"){ self.bump(); v.push(Stmt::Empty); }
            else { v.push(self.parse_expr_stmt()); }
        } Stmt::Block(v)
    }
    fn parse_var_decl(&mut self)->Stmt{
        let ty=self.bump().unwrap().text().to_string();
        let name= if self.cur_is(&TokenType::Identifier){ self.bump().unwrap().text().to_string() } else { self.error("expected identifier".into()); "_err".into() };
        let init= if self.cur_is_op("=") { self.bump(); Some(self.parse_expr()) } else { None };
        self.expect_punct(";"); Stmt::VarDecl{ ty, name, init }
    }
    fn parse_return(&mut self)->Stmt{
        self.expect_kw("return");
        if self.cur_is_punct(";"){ self.bump(); return Stmt::Return(None); }
        let e=self.parse_expr(); self.expect_punct(";"); Stmt::Return(Some(e))
    }
    fn parse_expr_stmt(&mut self)->Stmt{ let e=self.parse_expr(); self.expect_punct(";"); Stmt::ExprStmt(e) }

    fn parse_expr(&mut self)->Expr{ self.parse_binop(0) }
    fn precedence(op:&str)->i32{ match op { "||"=>1,"&&"=>2, "=="|"!="=>3, "<"|">"|"<="|">="=>4, "+"|"-"=>5, "*"|"/"|"%"=>6, _=>-1 } }
    fn parse_binop(&mut self, min_prec:i32)->Expr{
        let mut lhs=self.parse_unary();
        loop{
            let op = if let Some(t)=self.cur(){ if let TokenType::Operator=t.kind(){ t.text().to_string() } else { break; } } else { break; };
            let prec=Self::precedence(&op); if prec<min_prec { break; }
            if prec<0 { break; }
            self.bump();
            let mut rhs=self.parse_unary();
            loop {
                let next_op = if let Some(t)=self.cur(){ if let TokenType::Operator=t.kind(){ t.text().to_string() } else { break; } } else { break; };
                let next_prec=Self::precedence(&next_op);
                if next_prec>prec { rhs=self.parse_binop(next_prec); } else { break; }
            }
            lhs=Expr::Binary{ op, lhs:Box::new(lhs), rhs:Box::new(rhs) };
        } lhs
    }
    fn parse_unary(&mut self)->Expr{
        if self.cur_is_op("+") || self.cur_is_op("-") || self.cur_is_op("!") {
            let op=self.bump().unwrap().text().to_string(); let e=self.parse_unary(); return Expr::Unary{ op, expr: Box::new(e) };
        } self.parse_postfix()
    }
    fn parse_postfix(&mut self)->Expr{
        let mut e=self.parse_primary();
        loop{
            if self.cur_is_punct("("){
                self.bump(); let mut args=vec![];
                if !self.cur_is_punct(")"){
                    loop{ args.push(self.parse_expr()); if self.cur_is_punct(")"){ break; } self.expect_punct(","); }
                }
                self.expect_punct(")");
                if let Expr::Ident(name)=e { e=Expr::Call{ callee:name, args }; } else { self.error("call on non-identifier".into()); }
                continue;
            } break;
        } e
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
pub fn stringify_ast(stmts:&[Stmt])->String{
    fn indent(n:usize)->String{ "  ".repeat(n) }
    fn fmt_expr(e:&Expr, d:usize, out:&mut String){
        match e {
            Expr::Int(v)|Expr::Float(v)|Expr::Str(v)|Expr::Char(v)|Expr::Ident(v)=>out.push_str(&format!("{}",v)),
            Expr::Unary{op,expr}=>{ out.push_str(&format!("({} ",op)); fmt_expr(expr,d+1,out); out.push(')'); }
            Expr::Binary{op,lhs,rhs}=>{ out.push('('); fmt_expr(lhs,d+1,out); out.push_str(&format!(" {} ",op)); fmt_expr(rhs,d+1,out); out.push(')'); }
            Expr::Call{callee,args}=>{ out.push_str(&format!("{}(",callee)); for (i,a) in args.iter().enumerate(){ if i>0{out.push_str(", ");} fmt_expr(a,d+1,out);} out.push(')'); }
        }
    }
    fn fmt_stmt(s:&Stmt, d:usize, out:&mut String){
        match s {
            Stmt::VarDecl{ty,name,init}=>{ out.push_str(&format!("{}decl {} {}",indent(d),ty,name)); if let Some(e)=init{ out.push_str(" = "); fmt_expr(e,d,out);} out.push('\n'); }
            Stmt::Return(e)=>{ out.push_str(&format!("{}return",indent(d))); if let Some(e)=e{ out.push(' '); fmt_expr(e,d,out);} out.push('\n'); }
            Stmt::ExprStmt(e)=>{ out.push_str(&format!("{}expr ",indent(d))); fmt_expr(e,d,out); out.push('\n'); }
            Stmt::Block(v)=>{ out.push_str(&format!("{}block {{\n",indent(d))); for st in v { fmt_stmt(st,d+1,out);} out.push_str(&format!("{}}}\n",indent(d))); }
            Stmt::Empty=>{ out.push_str(&format!("{};\n",indent(d))); }
        }
    }
    let mut s=String::new(); for st in stmts { fmt_stmt(st,0,&mut s); } s
}
