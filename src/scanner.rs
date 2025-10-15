#[path = "token.rs"]
mod token;
use token::*;

#[path = "cstream.rs"]
mod cstream;
use cstream::*;

pub struct Scanner {
    char_stream: CStream,
    cur_str: String,
    token_vec: Vec<Token>,
}

impl Scanner {
    pub fn new(f: &str) -> Scanner {
        Scanner {
            char_stream: CStream::new(f),
            cur_str: "".to_string(),
            token_vec: Vec::new(),
        }
    }

    pub fn tokenize(&mut self) {
        // initialize file content
        self.char_stream.set_content();
        // initialize endline characters vec
        let endl_vec:Vec<char> = vec![' ', '\n','\t'];
        // initialize operators vec, exclude -, ==, <=, >=, !=, "-" maybe confused with constant type
        let operator_vec:Vec<char> = vec![ '(', ',', ')', '{', '}', '=', '<', '>', '+', '*', '/', ';'];
        // initialize keyword vec
        let keyword_vec:Vec<String> = vec!["unsigned".to_string(), "char".to_string(), "short".to_string(),
        "int".to_string(), "long".to_string(), "float".to_string(), "double".to_string(),
        "while".to_string(), "if".to_string(), "return".to_string(), "void".to_string(), "main".to_string()];
        // initialize digit vec
        let digit_vec:Vec<char> = vec!['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '-'];

        // initialize a new token
        let mut new_token = Token::new("".to_string(), TokenType::NONE, 0, 0);
        // initialize an empty string
        let mut string = "".to_string();
        // get first char from the file
        let mut c = self.char_stream.get_next_char().unwrap();

        // within a loop, identify all the tokens, splited by 
        // operators and endline characters
        while self.char_stream.more_available() {
            // split by operators and endl characters
            while !endl_vec.contains(&c) || !operator_vec.contains(&c) {
                string.push(c);
                c = self.char_stream.get_next_char().unwrap();
            } 

            // push spec character into string if string is empty
            if string.is_empty() {
                string.push(c);
            }

            if string.len() == 1 {
                // operator case exclude for ==, <=, >=, !=
                if operator_vec.contains(&string.chars().nth(0).unwrap()) || string.chars().nth(0).unwrap() == '-' {
                    new_token = Token::new(string, TokenType::OPERATOR, self.char_stream.get_line_number(), self.char_stream.get_char_pos());
                    self.token_vec.push(new_token);
                // endline character case, marked as type none
                } else if endl_vec.contains(&string.chars().nth(0).unwrap()) {
                    new_token = Token::new(string, TokenType::NONE, self.char_stream.get_line_number(), self.char_stream.get_char_pos());
                    self.token_vec.push(new_token);
                // constant case
                } else if string.chars().nth(0).unwrap().is_digit(10){
                    new_token = Token::new(string, TokenType::INTCONSTANT, self.char_stream.get_line_number(), self.char_stream.get_char_pos());
                    self.token_vec.push(new_token);
                // identifier case
                } else {
                    new_token = Token::new(string, TokenType::VARIABLE, self.char_stream.get_line_number(), self.char_stream.get_char_pos());
                    self.token_vec.push(new_token);
                }
            } else if string.len() == 2 {
                // operator case of ==, <=, >=, !=
                if string.chars().nth(1).unwrap() == '=' {
                    if string.chars().nth(0).unwrap() == '=' || string.chars().nth(0).unwrap() == '<' 
                    || string.chars().nth(0).unwrap() == '>' || string.chars().nth(0).unwrap() == '!' {
                        new_token = Token::new(string, TokenType::OPERATOR, self.char_stream.get_line_number(), self.char_stream.get_char_pos());
                        self.token_vec.push(new_token);   
                    }
                // constant case
                } else if string.chars().nth(0).unwrap().is_digit(10) && string.chars().nth(1).unwrap().is_digit(10) {
                    new_token = Token::new(string, TokenType::INTCONSTANT, self.char_stream.get_line_number(), self.char_stream.get_char_pos());
                    self.token_vec.push(new_token);
                // negative constant case
                } else if string.chars().nth(0).unwrap() == '-' && string.chars().nth(1).unwrap().is_digit(10) {
                    new_token = Token::new(string, TokenType::INTCONSTANT, self.char_stream.get_line_number(), self.char_stream.get_char_pos());
                    self.token_vec.push(new_token);
                // keyword case
                } else if keyword_vec.contains(&string) {
                    new_token = Token::new(string, TokenType::KEYWORD, self.char_stream.get_line_number(), self.char_stream.get_char_pos());
                    self.token_vec.push(new_token); 
                // identifier case
                } else {
                    new_token = Token::new(string, TokenType::VARIABLE, self.char_stream.get_line_number(), self.char_stream.get_char_pos());
                    self.token_vec.push(new_token);
                }
            } else if string.len() > 2 {
                // check keyword
                if keyword_vec.contains(&string) {
                    new_token = Token::new(string, TokenType::KEYWORD, self.char_stream.get_line_number(), self.char_stream.get_char_pos());
                    self.token_vec.push(new_token); 
                // check constant
                } else if string.chars().nth(0).unwrap().is_digit(10) || string.chars().nth(0).unwrap() == '-' {
                    let mut pushed = false;
                    for cha in string.chars() {
                        if digit_vec.contains(&cha) {
                            continue;
                        } else {
                            new_token = Token::new(string, TokenType::INVALID, self.char_stream.get_line_number(), self.char_stream.get_char_pos());
                            self.token_vec.push(new_token);
                            pushed = true;
                            break;
                        }
                    }
                // identifier case
                } else {
                    new_token = Token::new(string, TokenType::VARIABLE, self.char_stream.get_line_number(), self.char_stream.get_char_pos());
                    self.token_vec.push(new_token);
                }
            }

            // goto next char
            c = self.char_stream.get_next_char().unwrap();
            string = "".to_string();
        }
    }

    pub fn get_token_vec(&self) -> &Vec<Token>{
        &self.token_vec
    }

    pub fn print_token1(&self) {
        println!("{}", self.token_vec[1].get_text());
    }
} 