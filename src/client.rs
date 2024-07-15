use colored::Colorize;
use crossterm::cursor::MoveTo;
use crossterm::event::{poll, read, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{self, Clear, ClearType};
use std::io::{stdout, ErrorKind, Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use std::{env, str};

struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

fn chat_window(buffer: &mut String, chat: &[String], boundary: Rect) {
    let n = chat.len();
    let m = n.checked_sub(boundary.h).unwrap_or(0);

    for (dy, line) in chat.iter().skip(m).enumerate() {
        buffer.push_str(&format!(
            "{}{}",
            MoveTo(boundary.x as u16, (boundary.y + dy) as u16),
            &line[..line.len().min(boundary.w)]
        ));
    }
}

fn main() {
    let mut args = env::args();
    let _program = args.next().expect("program name");
    let ip = args.next().unwrap();

    let mut stream = TcpStream::connect(format!("{ip}:6969")).unwrap();
    stream.set_nonblocking(true).unwrap();

    let (mut w, mut h) = terminal::size().unwrap();

    terminal::enable_raw_mode().unwrap();
    let mut stdout = stdout();

    let bar_char = "═".white();
    let mut bar = bar_char.repeat(w as usize);
    let mut quit = false;
    let mut prompt = String::new();
    let mut chat = Vec::new();
    let mut buf = [0; 64];

    let quit_msg = "Exiting program. Goodbye!".bright_blue().bold();

    while !quit {
        // Event handling loop
        while poll(Duration::ZERO).unwrap() {
            match read().unwrap() {
                Event::Key(event) => match event.kind {
                    KeyEventKind::Press => match event.code {
                        KeyCode::Char(x) => {
                            if x == 'c' && event.modifiers.contains(KeyModifiers::CONTROL) {
                                chat.push(format!("{}", quit_msg));
                                quit = true;
                            } else {
                                prompt.push(x);
                            }
                        }
                        KeyCode::Enter => {
                            if !prompt.is_empty() {
                                let message = prompt.clone() + "\n";
                                stream.write_all(message.as_bytes()).unwrap();
                                chat.push(prompt.clone());
                                prompt.clear();
                            }
                        }
                        KeyCode::Backspace => {
                            prompt.pop();
                        }
                        _ => {}
                    },
                    _ => {}
                },
                Event::Paste(data) => {
                    prompt.push_str(&data);
                }
                Event::Resize(nw, nh) => {
                    w = nw;
                    h = nh;
                    bar = bar_char.repeat(w as usize);
                }
                _ => {}
            }
        }

        // Read stream
        match stream.read(&mut buf) {
            Ok(n) => {
                if n > 0 {
                    chat.push(str::from_utf8(&buf[0..n]).unwrap().to_string());
                }
            }
            Err(err) => {
                if err.kind() != ErrorKind::WouldBlock {
                    panic!("{err}");
                }
            }
        };

        // Create an off-screen buffer
        let mut buffer = String::new();
        buffer.push_str(&Clear(ClearType::All).to_string());

        chat_window(
            &mut buffer,
            &chat,
            Rect {
                x: 0,
                y: 0,
                w: w as usize,
                h: h as usize - 2,
            },
        );

        // Draw the bar
        buffer.push_str(&format!("{}{}", MoveTo(0, h - 2), bar));

        // Draw the prompt
        buffer.push_str(&format!(
            "{}{}",
            MoveTo(0, h - 1),
            &prompt[..prompt.len().min(w as usize)]
        ));

        // Write the buffer to stdout
        stdout.write_all(buffer.as_bytes()).unwrap();
        stdout.flush().unwrap();

        // Delay to control refresh rate
        thread::sleep(Duration::from_millis(33));
    }

    terminal::disable_raw_mode().unwrap();
}
