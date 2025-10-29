mod token; mod cstream; mod scanner;
use scanner::*; use token::*;
use std::fs::File; use std::io::{Write, Read}; use std::env;

fn html_escape(s:&str)->String{
    s.chars().map(|c|match c{
        '&'=>"&amp;".into(), '<'=>"&lt;".into(), '>'=>"&gt;".into(),
        '"'=>"&quot;".into(), '\''=>"&#39;".into(), _=>c.to_string(),
    }).collect::<Vec<_>>().join("")
}
fn css(theme:&str)->String{
    let (bg,fg,c_kw,c_id,c_num,c_str,c_op,c_punct,c_cmt,c_err,c_pp)=match theme{
        "light"=>("#ffffff","#24292e","#cf222e","#8250df","#116329","#0a3069","#953800","#57606a","#6e7781","#b31d28","#0550ae"),
        _=>("#0d1117","#e6edf3","#ff7b72","#d2a8ff","#79c0ff","#a5d6ff","#ffa657","#c9d1d9","#8b949e","#b62324","#2f81f7"),
    };
    format!("
body{{background:{bg};color:{fg};font:14px/1.6 ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace; padding:24px;}}
.code{{display:grid; grid-template-columns: auto 1fr; gap: 12px;}}
.gutter{{user-select:none; color:#6e7681; text-align:right; padding-right:8px;}}
pre{{white-space:pre-wrap; word-break:break-word; margin:0;}}
.kw{{color:{c_kw};font-weight:600}} .id{{color:{c_id}}} .num{{color:{c_num}}}
.str{{color:{c_str}}} .op{{color:{c_op}}} .punct{{color:{c_punct}}}
.cmt{{color:{c_cmt}}} .pp{{color:{c_pp}}} .ws{{white-space:pre-wrap}}
.err{{background:{c_err};color:#fff;border-radius:4px;padding:0 2px}}
.tok{{color:{c_punct}}} .legend span{{margin-right:12px}}
", bg=bg,fg=fg,c_kw=c_kw,c_id=c_id,c_num=c_num,c_str=c_str,c_op=c_op,c_punct=c_punct,c_cmt=c_cmt,c_err=c_err,c_pp=c_pp)
}
fn serde_json_escape(s:&str)->String{
    let mut out=String::with_capacity(s.len()+2); out.push('"');
    for ch in s.chars(){ match ch{
        '"'=>out.push_str("\\""), '\\'=>out.push_str("\\\\"),
        '\n'=>out.push_str("\\n"), '\r'=>out.push_str("\\r"), '\t'=>out.push_str("\\t"),
        c if c.is_control()=>out.push_str(&format!("\\u{:04x}", c as u32)), c=>out.push(c),
    }}
    out.push('"'); out
}
fn main(){
    let args:Vec<String>=env::args().collect();
    let mut input="example1.x".to_string();
    let mut out="out.html".to_string();
    let mut theme="dark".to_string();
    let mut dump=false; let mut no_ws=false; let mut no_comments=false; let mut json_out=false; let mut kw_file:Option<String>=None; let mut stats=false;
    let mut i=1; while i<args.len(){
        match args[i].as_str(){
            "--in" if i+1<args.len()=>{input=args[i+1].clone(); i+=1;}
            "--out" if i+1<args.len()=>{out=args[i+1].clone(); i+=1;}
            "--theme" if i+1<args.len()=>{theme=args[i+1].clone(); i+=1;}
            "--dump"=>{dump=true;} "--no-ws"=>{no_ws=true;} "--no-comments"=>{no_comments=true;} "--json"=>{json_out=true;} "--stats"=>{stats=true;}
            "--kw" if i+1<args.len()=>{kw_file=Some(args[i+1].clone()); i+=1;}
            _=>{}
        } i+=1;
    }
    let mut s=Scanner::new(&input);
    if let Some(path)=kw_file{
        let mut buf=String::new(); if let Ok(mut f)=File::open(&path){ let _=f.read_to_string(&mut buf); let kws=buf.split_whitespace().map(|w|w.to_string()).collect::<Vec<_>>(); if !kws.is_empty(){ s.set_keywords(kws); } }
    }
    s.tokenize();
    if stats{
        let map=s.token_stats(); eprintln!("Token stats:"); let mut v:Vec<_>=map.into_iter().collect(); v.sort_by_key(|(k,_)|k.to_string()); for (k,c) in v{ eprintln!("{:>12}: {}", k, c); }
    }
    if json_out{
        print!("[");
        let mut first=true;
        for t in s.get_token_vec(){
            if no_comments && matches!(t.kind(), TokenType::Comment|TokenType::Whitespace){ continue; }
            if !first{ print!(","); } first=false;
            print!("{{"kind":"{:?}","text":{},"start":{{"line":{},"col":{}}},"end":{{"line":{},"col":{}}}}}",
                t.kind(), serde_json_escape(t.text()), t.start().line, t.start().col, t.end().line, t.end().col);
        }
        println!("]");
        if !s.diagnostics().is_empty(){ eprintln!("Diagnostics:"); for d in s.diagnostics(){ eprintln!("{}:{} {}", d.start.line, d.start.col, d.message); } }
        return;
    }
    if dump{
        for t in s.get_token_vec(){
            if no_comments && matches!(t.kind(), TokenType::Comment|TokenType::Whitespace){ continue; }
            println!("{:?}\t{:?}\t{}:{}", t.kind(), t.text(), t.start().line, t.start().col);
        }
        if !s.diagnostics().is_empty(){ eprintln!("Diagnostics:"); for d in s.diagnostics(){ eprintln!("  {}:{}  {}", d.start.line, d.start.col, d.message); } }
    }
    let mut file=File::create(&out).expect("cannot create out.html");
    let head=format!("<!doctype html><html><head><meta charset='utf-8'><title>X Highlighter</title><style>{}</style></head><body><div class='legend'><span class='kw'>kw</span><span class='id'>id</span><span class='num'>num</span><span class='str'>str</span><span class='op'>op</span><span class='punct'>punct</span><span class='cmt'>cmt</span><span class='pp'>pp</span><span class='err'>err</span></div><hr/><div class='code'><pre class='gutter'>", css(&theme));
    file.write_all(head.as_bytes()).unwrap();
    let max_line=s.get_token_vec().iter().filter(|t| !(no_comments && matches!(t.kind(), TokenType::Comment))).map(|t|t.start().line).max().unwrap_or(1);
    for i in 1..=max_line{ file.write_all(format!("{:>4}\n", i).as_bytes()).unwrap(); }
    file.write_all(b"</pre>\n<pre>\n").unwrap();
    for t in s.get_token_vec(){
        if no_comments && matches!(t.kind(), TokenType::Comment){ continue; }
        let cls=t.css_class(); let txt=html_escape(t.text());
        if matches!(t.kind(), TokenType::Whitespace){
            if no_ws{ file.write_all(txt.replace('\n',"\n").as_bytes()).unwrap(); } else { file.write_all(txt.as_bytes()).unwrap(); }
        } else {
            file.write_all(format!("<span class='{}' title='{}:{}'>{}</span>", cls, t.start().line, t.start().col, txt).as_bytes()).unwrap();
        }
    }
    file.write_all(b"\n</pre>\n</div>\n").unwrap();
    if !s.diagnostics().is_empty(){
        file.write_all(b"<div class='diag'>\n").unwrap();
        file.write_all(format!("<div><b>Diagnostics ({}):</b></div>\n", s.diagnostics().len()).as_bytes()).unwrap();
        for d in s.diagnostics(){ file.write_all(format!("<div class='item'>{}:{} — {}</div>\n", d.start.line, d.start.col, html_escape(&d.message)).as_bytes()).unwrap(); }
        file.write_all(b"</div>\n").unwrap();
    }
    file.write_all(b"</body></html>\n").unwrap();
    println!("Wrote {}", out);
}
