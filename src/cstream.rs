use std::fs::File;
use std::io;
use std::io::prelude::*;


pub struct CStream {
	filename: String,
	line_num: i32,
	char_pos: i32,
	content: String,
	overall_pos: i32,
	size: i32
}


impl CStream {
    pub fn new(f: &str) -> CStream {
        CStream {
            filename: f.to_string(),
			line_num: -1,
			char_pos: -1,
            content: String::new(),
            overall_pos: -1,
            size: 0
        }
    }


    pub fn set_content(&mut self) -> io::Result<()> {
        let file = File::open(self.filename.as_str())?;
        let mut buf_reader = io::BufReader::new(file);
        buf_reader.read_to_string(&mut self.content);
        self.size = self.content.chars().count() as i32;
        Ok(())
    }

	pub fn more_available(&self) -> bool {
		self.overall_pos < self.size
	}

	// Rust is able to handle we are out of bounds
	// nth() returns an Option<char>, which is None when out of bounds
	pub fn get_cur_char(&self) -> Option<char> {
		self.content.chars().nth((self.overall_pos) as usize)
	}

	pub fn get_next_char(&mut self) -> Option<char> {
		let cur = self.content.chars().nth((self.overall_pos) as usize);
		match cur {
			None => {
				self.char_pos += 1;
				self.line_num += 1;
			},
			Some(x) => {
				if x == '\n' {
					self.char_pos = 0;
					self.line_num += 1;
				} else {
					self.char_pos += 1;
					if self.overall_pos == -1 {
						self.line_num += 1;
					}
				}
			}
		}
		self.overall_pos += 1;
		self.content.chars().nth((self.overall_pos) as usize)
	}

	pub fn peek_next_char(&self) -> Option<char> {
		self.content.chars().nth((self.overall_pos+1) as usize)
	}

	pub fn peek_ahead_char(&self, k: i32) -> Option<char> {
		self.content.chars().nth((self.overall_pos+k+1) as usize)
	}
	
	pub fn get_line_number(&self) -> i32 {
		self.line_num
	}

	pub fn get_char_pos(&self) -> i32 {
		self.char_pos
	}
}
