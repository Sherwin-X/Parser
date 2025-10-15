mod token;
mod cstream;
mod scanner;
use scanner::*;

fn main() {
    let mut s = Scanner::new("example1.x");
    let mut v:Vec<Token> = Vec::new();
    s.tokenize();
    v = s.get_token_vec();

    // Head
    file.write_all(r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd">"#.as_bytes()).expect("write failed");
    file.write_all("\n".as_bytes()).expect("write failed");
    file.write_all(r#"<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">"#.as_bytes()).expect("write failed");
    file.write_all("\n<head>\n<title>\nX Formatted file</title>\n</head>\n".as_bytes()).expect("write failed");
    file.write_all(r#"<body bgcolor="navy" text="orange" link="orange" vlink="orange">"#.as_bytes()).expect("write failed");
    file.write_all("\n".as_bytes()).expect("write failed");
    file.write_all(r#"<font face="Courier New">"#.as_bytes()).expect("write failed");
    file.write_all("\n".as_bytes()).expect("write failed");
    /* pseudo code

    for j in range(total line num of file):
        for i in TokenVector(each line of token, use two dimension vector if available):
            if i == " ":
                file.write_all(r#"&nbsp;"#.as_bytes()).expect("write failed");
            else:
                if Tokentype.i is Identifier:
                    color = yellow
                    bold = 0
                elseif Tokentype.i is Float constant or Int constant:
                    color = aqua
                    bold = 1
                else:
                    color = white
                    bold = 1
                if bold = 0:
                    file.write_all(r#"font color="color">i</font>"#.as_bytes()).expect("write failed");
                elseif bold = 1:
                    file.write_all(r#"<font color="color"><b>i</b></font>"#.as_bytes()).expect("write failed");
        j++
        file.write_all("<br />".as_bytes()).expect("write failed");(html endline)
        file.write_all("\n".as_bytes()).expect("write failed");(txt endline)

    */

    // Body actual code

    // Body for test
    file.write_all(r#" <font color="white"><b>float</b></font> <a name="f0Foo"/><font color="yellow">Foo</font><font color="white"><b>(</b></font><font color="white"><b>int</b></font> <font color="yellow">val</font><font color="white"><b>)</b></font><font color="white"><b>;</b></font><br />"#.as_bytes()).expect("write failed");
    file.write_all("\n".as_bytes()).expect("write failed");
    file.write_all(r#"<font color="white"><b>void</b></font> <font color="white"><b>main</b></font><font color="white"><b>(</b></font><font color="white"><b>)</b></font><font color="white"><b>{</b></font><br />"#.as_bytes()).expect("write failed");
    file.write_all("\n".as_bytes()).expect("write failed");
    file.write_all(r#"&nbsp;&nbsp; &nbsp;<font color="white"><b>float</b></font> <a name="v1Value"/><font color="yellow">Value</font><font color="white"><b>;</b></font><br />"#.as_bytes()).expect("write failed");

    // Tail
    file.write_all("\n</font>\n</body>\n</html>\n".as_bytes()).expect("write failed");

}