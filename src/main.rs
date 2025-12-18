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
use token::TokenType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DumpTokensMode {
    Off,
    Compact,
    Full,
}

#[derive(Debug)]
struct Cli {
    dump_tokens: DumpTokensMode,
    dump_ast: bool,
    source_path: Option<String>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            dump_tokens: DumpTokensMode::Off,
            dump_ast: false,
            source_path: None,
        }
    }
}

fn print_usage(program: &str) {
    eprintln!(
        "Usage: {prog} [options] [source_file]\n\
         \n\
         Options:\n\
         \t--dump-tokens[=compact|full]   Print tokens (escaped). Default: compact.\n\
         \t--dump-ast                     Print parsed AST using Rust Debug format.\n\
         \t-h, --help                     Show this help message.\n\
         \n\
         If no source_file is given, defaults to 'example1.x'.",
        prog = program
    );
}

fn parse_args(program: &str) -> Cli {
    let mut args = env::args().collect::<Vec<_>>();
    args.remove(0); // drop program

    let mut cli = Cli::default();

    for a in args {
        match a.as_str() {
            "-h" | "--help" => {
                print_usage(program);
                process::exit(0);
            }
            "--dump-ast" => cli.dump_ast = true,

            "--dump-tokens" => {
                cli.dump_tokens = DumpTokensMode::Compact;
            }

            _ if a.starts_with("--dump-tokens=") => {
                let mode = a.trim_start_matches("--dump-tokens=");
                cli.dump_tokens = match mode {
                    "compact" => DumpTokensMode::Compact,
                    "full" => DumpTokensMode::Full,
                    _ => {
                        eprintln!("Invalid value for --dump-tokens: '{mode}'\n");
                        print_usage(program);
                        process::exit(2);
                    }
                };
            }

            _ if a.starts_with('-') => {
                eprintln!("Unknown option: {a}\n");
                print_usage(program);
                process::exit(2);
            }

            _ => {
                if cli.source_path.is_none() {
                    cli.source_path = Some(a);
                } else {
                    eprintln!("Unexpected extra argument: {a}\n");
                    print_usage(program);
                    process::exit(2);
                }
            }
        }
    }

    cli
}

/// Escape token text so dump output is stable (no raw newlines/tabs/control chars).
fn escape_token_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c if c.is_control() => {
                // control chars -> \u{..}
                out.push_str(&format!("\\u{{{:X}}}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

fn dump_tokens(tokens: &[token::Token], mode: DumpTokensMode) {
    println!("== Tokens ==");
    match mode {
        DumpTokensMode::Off => {}
        DumpTokensMode::Compact => {
            for t in tokens {
                let sp = t.span();
                let kind = format!("{:?}", t.kind());
                let text = t.text();
                let shown = if text.is_empty() {
                    "<EOF>".to_string()
                } else {
                    format!("\"{}\"", escape_token_text(text))
                };
                println!("{:>4}:{:<3}  {:<14}  {}", sp.line, sp.col, kind, shown);
            }
        }
        DumpTokensMode::Full => {
            for t in tokens {
                let sp = t.span();
                let kind = format!("{:?}", t.kind());
                let text = t.text();
                let shown = if text.is_empty() {
                    "<EOF>".to_string()
                } else {
                    format!("\"{}\"", escape_token_text(text))
                };
                println!(
                    "{:>4}:{:<3}  {:<14}  idx={:<6} len={:<4}  {}",
                    sp.line, sp.col, kind, sp.idx, sp.len, shown
                );
            }
        }
    }
}

fn main() {
    let program = env::args().next().unwrap_or_else(|| "compiler".to_string());
    let cli = parse_args(&program);

    // 默认文件
    let path = cli.source_path.unwrap_or_else(|| "example1.x".to_string());
    let path = Path::new(&path);

    // 读取源码
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read '{}': {}", path.display(), e);
            process::exit(2);
        }
    };

    // 词法分析
    let (tokens, lex_errors) = Scanner::new(&source).scan();

    if cli.dump_tokens != DumpTokensMode::Off {
        dump_tokens(&tokens, cli.dump_tokens);
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
    if tokens.len() <= 1
        && tokens
            .first()
            .map(|t| matches!(t.kind(), TokenType::Eof))
            .unwrap_or(false)
    {
        if had_error {
            process::exit(1);
        }
        return;
    }

    // 语法分析
    let mut parser = Parser::new(tokens, &source);
    let items = parser.parse_items();

    // 打印语法错误
    if !parser.errors.is_empty() {
        had_error = true;
        eprintln!("== Parser Errors ({}) ==\n", parser.errors.len());
        for e in &parser.errors {
            eprintln!("{}", e.render());
        }
    }

    // 可选：打印 AST（Debug）
    if cli.dump_ast {
        println!("== AST (Debug) ==");
        println!("{:#?}", items);
        println!();
    }

    // 无错误则打印 pretty 输出
    if !had_error {
        let pretty = stringify_items(&items);
        println!("{}", pretty);
    } else {
        process::exit(1);
    }
}
