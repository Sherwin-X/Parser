// main.rs
mod token;
mod scanner;
mod parser;

use std::env;
use std::fs;
use std::path::Path;
use std::process;

use scanner::Scanner;
use parser::{Parser, stringify_items};
use token::{TokenType};

fn print_usage(program: &str) {
    eprintln!(
        "Usage: {prog} [--dump-tokens] [source_file]\n\
         \n\
         Options:\n\
         \t--dump-tokens   Print tokens with span info.\n\
         \n\
         If no source_file is given, defaults to 'example1.x'.",
        prog = program
    );
}

fn dump_tokens(tokens: &[token::Token]) {
    println!("== Tokens ==");
    for t in tokens {
        let sp = t.span();
        let kind = format!("{:?}", t.kind());
        let text = t.text();
        println!("{:>4}:{:<3}  {:<14}  {}", sp.line, sp.col, kind, if text.is_empty() { "<EOF>" } else { text });
    }
}

fn main() {
    let mut args = env::args().collect::<Vec<_>>();
    let program = args.remove(0);

    let mut dump = false;
    let mut source_path: Option<String> = None;

    for a in args {
        match a.as_str() {
            "-h" | "--help" => {
                print_usage(&program);
                return;
            }
            "--dump-tokens" => { dump = true; }
            _ => {
                if source_path.is_none() {
                    source_path = Some(a);
                } else {
                    eprintln!("Unexpected extra argument: {}", a);
                    print_usage(&program);
                    process::exit(2);
                }
            }
        }
    }

    let path = source_path.unwrap_or_else(|| "example1.x".to_string());

    // 读取源码
    let source = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read '{}': {}", path, e);
            process::exit(2);
        }
    };

    // 词法分析
    let (tokens, lex_errors) = Scanner::new(&source).scan();

    // 可选：打印 tokens
    if dump {
        dump_tokens(&tokens);
        println!();
    }

    // 打印词法错误
    let mut had_error = false;
    if !lex_errors.is_empty() {
        had_error = true;
        eprintln!("== Lexer Errors ({}) ==\n", lex_errors.len());
        for e in &lex_errors {
            eprintln!("{}", e.render());
        }
    }

    // 若只有 EOF 一个 token，则无需继续
    if tokens.len() <= 1 && tokens.first().map(|t| matches!(t.kind(), TokenType::Eof)).unwrap_or(false) {
        if had_error {
            process::exit(1);
        } else {
            println!("(empty input)");
            return;
        }
    }

    // 语法分析
    let mut parser = Parser::new(tokens, source.clone());
    let items = parser.parse_items();

    // 打印语法错误
    if !parser.errors.is_empty() {
        had_error = true;
        eprintln!("== Parser Errors ({}) ==\n", parser.errors.len());
        for e in &parser.errors {
            eprintln!("{}", e.render());
        }
    }

    // 无错误则打印 pretty 输出
    if !had_error {
        let pretty = stringify_items(&items);
        println!("{}", pretty);
    } else {
        // 有错误，退出非零
        process::exit(1);
    }
}
