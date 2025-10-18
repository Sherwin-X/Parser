mod token;
mod cstream;
mod scanner;
use scanner::*;
use token::*;

fn main() {
    let mut s = Scanner::new("example1.x");
    s.tokenize();
    for t in s.get_token_vec() {
        println!("{:?}\t{:?}\t{}:{}", t.kind(), t.text(), t.line(), t.col());
    }
}
