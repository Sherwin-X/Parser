mod token;
mod cstream;
mod scanner;
use scanner::*;
use token::*;
use std::fs::File;
use std::io::Write;

fn html_escape(s: &str) -> String {
    s.chars().map(|c| match c {
        '&' => "&amp;".into(),
        '<' => "&lt;".into(),
        '>' => "&gt;".into(),
        '"' => "&quot;".into(),
        '\'' => "&#39;".into(),
        _ => c.to_string(),
    }).collect::<Vec<_>>().join("")
}

fn main() {
    let mut s = Scanner::new("example1.x");
    s.tokenize();
    for t in s.get_token_vec() {
        println!("{:?}\t{:?}\t{}:{}", t.kind(), t.text(), t.line(), t.col());
    }

    let mut file = File::create("out.html").expect("cannot create out.html");
    let head = r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>X Highlighter</title>
<style>
 body{background:#0d1117;color:#e6edf3;font:14px/1.6 ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace; padding:16px;}
 pre{white-space:pre-wrap; word-break:break-word;}
 .kw{color:#ff7b72;font-weight:600}
 .id{color:#d2a8ff}
 .num{color:#79c0ff}
 .str{color:#a5d6ff}
 .op{color:#ffa657}
 .punct{color:#c9d1d9}
 .cmt{color:#8b949e}
 .ws{white-space:pre-wrap}
 .err{background:#b62324;color:#fff;border-radius:4px;padding:0 2px}
 .tok{color:#c9d1d9}
</style>
</head>
<body>
<pre>
"#;
    file.write_all(head.as_bytes()).unwrap();

    for t in s.get_token_vec() {
        let cls = t.css_class();
        let txt = html_escape(t.text());
        if matches!(t.kind(), TokenType::Whitespace) {
            file.write_all(txt.as_bytes()).unwrap();
        } else {
            let span = format!("<span class=\"{}\">{}</span>", cls, txt);
            file.write_all(span.as_bytes()).unwrap();
        }
    }

    file.write_all(b"\n</pre>\n</body>\n</html>\n").unwrap();
    println!("Wrote out.html");
}
