use colored::Colorize;
use crossterm::cursor::MoveTo;
use crossterm::event::{poll, read, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{self, Clear, ClearType};
use std::io::{stdout, ErrorKind, Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use std::{env, process, str};

struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

fn chat_window(buffer: &mut String, chat: &[String], boundary: Rect, offset: usize) {
    let n = chat.len();
    let m = n.checked_sub(boundary.h + offset).unwrap_or(0);

    for (dy, line) in chat.iter().skip(m + offset).take(boundary.h).enumerate() {
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
    let ip = args.next().unwrap_or_else(|| {
        eprintln!("Usage: <program> <IP>");
        process::exit(1);
    });

    let mut stream = TcpStream::connect(format!("{ip}:6969")).unwrap_or_else(|e| {
        eprintln!("Failed to connect: {}", e);
        process::exit(1);
    });
    stream.set_nonblocking(true).unwrap();

    let (mut w, mut h) = terminal::size().unwrap_or((80, 24));

    terminal::enable_raw_mode().unwrap();
    let mut stdout = stdout();

    let bar_char = "█".on_white();
    let mut bar = bar_char.repeat(w as usize);
    let mut quit = false;
    let mut prompt = String::new();
    let mut chat = Vec::new();
    let mut buf = [0; 64];
    let mut scroll_offset = 0;

    let quit_msg = "Exiting program. Goodbye!".bright_blue().bold();
    let title = "Tchat";

    // Buffer - last state
    let mut last_buffer = String::new();

    while !quit {
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
                        KeyCode::Up => {
                            if scroll_offset < chat.len() {
                                scroll_offset += 1;
                            }
                        }
                        KeyCode::Down => {
                            if scroll_offset > 0 {
                                scroll_offset -= 1;
                            }
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

        match stream.read(&mut buf) {
            Ok(n) => {
                if n > 0 {
                    chat.push(str::from_utf8(&buf[0..n]).unwrap().to_string());
                }
            }
            Err(err) => {
                if err.kind() != ErrorKind::WouldBlock {
                    eprintln!("Read error: {}", err);
                    process::exit(1);
                }
            }
        };

        let mut buffer = String::new();
        buffer.push_str(&Clear(ClearType::All).to_string());

        chat_window(
            &mut buffer,
            &chat,
            Rect {
                x: 0,
                y: 1,
                w: w as usize,
                h: h as usize - 3,
            },
            scroll_offset,
        );

        // Draw the top bar with title
        buffer.push_str(&format!(
            "{}{}{}{}",
            MoveTo(0, 0),
            bar,
            MoveTo(1, 0),
            title.black().on_white()
        ));

        // Draw the bar at the bottom
        buffer.push_str(&format!("{}{}", MoveTo(0, h - 2), bar));

        // Draw the prompt
        buffer.push_str(&format!(
            "{}{}",
            MoveTo(0, h - 1),
            &prompt[..prompt.len().min(w as usize)]
        ));

        if buffer != last_buffer {
            stdout.write_all(buffer.as_bytes()).unwrap();
            stdout.flush().unwrap();
            last_buffer = buffer;
        }

        thread::sleep(Duration::from_millis(33));
    }

    terminal::disable_raw_mode().unwrap();
}
