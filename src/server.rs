use colored::Colorize;
use getrandom::getrandom;
use std::{
    collections::HashMap,
    fmt::{self, Write as OtherWrite},
    io::{Read, Write},
    net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream},
    result, str,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
    thread,
    time::{Duration, SystemTime},
};

type Result<T> = result::Result<T, ()>;

const PORT: u16 = 6969;
const SAFE_MODE: bool = false;
const BAN_LIMIT: Duration = Duration::from_secs(10 * 60);
const MESSAGE_RATE: Duration = Duration::from_secs(1);
const STRIKE_LIMIT: i32 = 10;

struct Sens<T>(T);

impl<T: fmt::Display> fmt::Display for Sens<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(inner) = self;
        if SAFE_MODE {
            writeln!(f, "[REDACTED]")
        } else {
            inner.fmt(f)
        }
    }
}

fn print_error<T: fmt::Display>(message: T) {
    eprintln!("{}: {}", "ERROR".bold().red(), message);
}

fn print_info<T: fmt::Display>(message: T) {
    println!("{}: {}", "INFO".bold().truecolor(99, 105, 132), message);
}

enum Message {
    ClientConnected {
        author: Arc<TcpStream>,
    },
    ClientDisconnected {
        author_addr: SocketAddr,
    },
    NewMessage {
        author_addr: SocketAddr,
        bytes: Vec<u8>,
    },
}

struct Client {
    conn: Arc<TcpStream>,
    last_message: SystemTime,
    strike_count: i32,
    authed: bool,
}

fn server(messages: Receiver<Message>, token: String) -> Result<()> {
    let mut clients = HashMap::<SocketAddr, Client>::new();
    let mut banned_mfs = HashMap::<IpAddr, SystemTime>::new();
    loop {
        let msg = messages.recv().expect("The server receiver is not hung up");
        match msg {
            Message::ClientConnected { author } => {
                let author_addr = author
                    .peer_addr()
                    .expect("TODO: cache the peer addrs of the connection");
                let mut banned_at = banned_mfs.remove(&author_addr.ip());
                let now = SystemTime::now();

                banned_at = banned_at.and_then(|banned_at| {
                    let diff = now
                        .duration_since(banned_at)
                        .expect("TODO: don't crash if the clock went backwards");
                    if diff >= BAN_LIMIT {
                        None
                    } else {
                        Some(banned_at)
                    }
                });

                if let Some(banned_at) = banned_at {
                    let diff = now
                        .duration_since(banned_at)
                        .expect("TODO: don't crash if the clock went backwards");
                    banned_mfs.insert(author_addr.ip().clone(), banned_at);
                    let mut author = author.as_ref();
                    let secs = (BAN_LIMIT - diff).as_secs_f32();
                    print_info(format!(
                        "Client {author_addr} tried to connect, who is banned for {secs} secs"
                    ));
                    let _ =
                        writeln!(author, "You are banned MF: {secs} secs left",).map_err(|err| {
                            print_error(format!(
                                "could not send banned message to {author_addr}: {err}"
                            ))
                        });
                    let _ = author.shutdown(Shutdown::Both).map_err(|err| {
                        print_error(format!(
                            "could not shut down socket for {author_addr}: {err}"
                        ))
                    });
                } else {
                    print_info(format!("Client {author_addr} connected"));
                    clients.insert(
                        author_addr.clone(),
                        Client {
                            conn: author.clone(),
                            last_message: now - 2 * MESSAGE_RATE,
                            strike_count: 0,
                            authed: false,
                        },
                    );
                    let token_str = "Please enter the Token:".bright_yellow().underline().bold();
                    let _ = write!(author.as_ref(), "{}", token_str).map_err(|err| {
                        print_error(format!(
                            "could not send Token prompt to {}: {}",
                            Sens(author_addr),
                            Sens(err)
                        ));
                    });
                }
            }
            Message::ClientDisconnected { author_addr } => {
                print_info(format!("Client {author_addr} disconnected"));
                clients.remove(&author_addr);
            }
            Message::NewMessage { author_addr, bytes } => {
                if let Some(author) = clients.get_mut(&author_addr) {
                    let now = SystemTime::now();
                    let diff = now
                        .duration_since(author.last_message)
                        .unwrap_or_else(|err| {
                            print_error(format!("message rate check on new message: the clock might have gone backwards: {err}"));
                            Duration::from_secs(0)
                        });
                    if diff >= MESSAGE_RATE {
                        if let Ok(text) = str::from_utf8(&bytes) {
                            author.last_message = now;
                            author.strike_count = 0;

                            let bytes_without_last = if !bytes.is_empty() {
                                &bytes[..bytes.len() - 1]
                            } else {
                                &bytes[..]
                            };

                            if author.authed {
                                print_info(format!(
                                    "Client {author_addr} sent message {bytes_without_last:?}"
                                ));
                                for (addr, client) in clients.iter() {
                                    if *addr != author_addr && client.authed {
                                        let _ = writeln!(client.conn.as_ref(), "{text}").map_err(|err| {
                                        print_error(format!("could not broadcast message to all the clients from {author_addr}: {err}"))
                                    });
                                    }
                                }
                            } else {
                                let trimmed_text = text.trim();
                                let invalid_token_msg =
                                    "Invalid token!, disconnecting in 3.. 2.. 1..".red().bold();
                                if trimmed_text == token {
                                    author.authed = true;
                                    let welcome_msg = "Welcome to the Club!".green().bold();
                                    let _ = writeln!(author.conn.as_ref(), "{}", welcome_msg)
                                        .map_err(|err| {
                                            print_error(format!(
                                                "could not send welcome message to {}: {}",
                                                Sens(author_addr),
                                                Sens(err)
                                            ));
                                        });
                                } else {
                                    print_info(format!(
                                        "{} failed authorization!",
                                        Sens(author_addr)
                                    ));
                                    let _ = writeln!(author.conn.as_ref(), "{}", invalid_token_msg)
                                        .map_err(|err| {
                                            print_error(format!(
                                            "could not notify client {} about invalid token: {}",
                                            Sens(author_addr),
                                            Sens(err)
                                        ));
                                        });
                                    let _ = author.conn.shutdown(Shutdown::Both).map_err(|err| {
                                        print_error(format!(
                                            "could not shutdown {}: {}",
                                            Sens(author_addr),
                                            Sens(err)
                                        ))
                                    });
                                    clients.remove(&author_addr);
                                }
                            }
                        } else {
                            author.strike_count += 1;
                            if author.strike_count >= STRIKE_LIMIT {
                                print_info(format!("Client {author_addr} got banned"));
                                banned_mfs.insert(author_addr.ip().clone(), now);
                                let _ = writeln!(author.conn.as_ref(), "You are banned MF")
                                    .map_err(|err| {
                                        print_error(format!(
                                            "could not send banned message to {author_addr}: {err}"
                                        ))
                                    });
                                let _ = author.conn.shutdown(Shutdown::Both).map_err(|err| {
                                    print_error(format!(
                                        "could not shutdown socket for {author_addr}: {err}"
                                    ))
                                });
                            }
                        }
                    } else {
                        author.strike_count += 1;
                        if author.strike_count >= STRIKE_LIMIT {
                            print_info(format!("Client {author_addr} got banned"));
                            banned_mfs.insert(author_addr.ip().clone(), now);
                            let _ = writeln!(author.conn.as_ref(), "You are banned MF").map_err(
                                |err| {
                                    print_error(format!(
                                        "could not send banned message to {author_addr}: {err}"
                                    ))
                                },
                            );
                            let _ = author.conn.shutdown(Shutdown::Both).map_err(|err| {
                                print_error(format!(
                                    "could not shutdown socket for {author_addr}: {err}"
                                ))
                            });
                        }
                    }
                }
            }
        }
    }
}

fn client(stream: Arc<TcpStream>, messages: Sender<Message>) -> Result<()> {
    let author_addr = stream.peer_addr().map_err(|err| {
        print_error(format!("could not get peer address: {err}"));
    })?;

    messages
        .send(Message::ClientConnected {
            author: stream.clone(),
        })
        .map_err(|err| {
            print_error(format!(
                "could not sent message from {author_addr} to the server thread: {err}",
                author_addr = Sens(author_addr),
                err = Sens(err)
            ))
        })?;

    let mut buffer = Vec::new();
    loop {
        let mut temp_buffer = [0; 512]; // Temporary buffer for reading data
        let n = stream.as_ref().read(&mut temp_buffer).map_err(|err| {
            print_error(format!("could not read message from client: {err}"));
            let _ = messages
                .send(Message::ClientDisconnected { author_addr })
                .map_err(|err| {
                    print_error(format!(
                        "could not sent message to the server thread: {err}"
                    ))
                });
        })?;
        if n > 0 {
            buffer.extend_from_slice(&temp_buffer[..n]);
            if let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                let complete_message = buffer.drain(..=pos).collect::<Vec<_>>();
                messages
                    .send(Message::NewMessage {
                        author_addr,
                        bytes: complete_message,
                    })
                    .map_err(|err| {
                        print_error(format!(
                            "could not send message to the server thread: {err}"
                        ));
                    })?;
            }
        } else {
            let _ = messages
                .send(Message::ClientDisconnected { author_addr })
                .map_err(|err| {
                    print_error(format!(
                        "could not sent message to the server thread: {err}"
                    ))
                });
            break;
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let mut buffer: [u8; 16] = [0; 16];
    let _ = getrandom(&mut buffer).map_err(|err| {
        print_error(format!("could not generate random access token: {err}"));
    });

    let mut token = String::new();
    for x in buffer.iter() {
        let _ = write!(&mut token, "{x:02X}");
    }

    println!("Token: {token}");

    let address = format!("0.0.0.0:{PORT}");
    let listener = TcpListener::bind(&address).map_err(|err| {
        print_error(format!("could not bind {address}: {}", Sens(err)));
    })?;
    print_info(format!("listening to address: {}", address));

    let (message_sender, message_receiver) = channel();
    thread::spawn(|| server(message_receiver, token));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let stream = Arc::new(stream);
                let message_sender = message_sender.clone();
                thread::spawn(|| client(stream, message_sender));
            }
            Err(err) => {
                print_error(format!("could not accept connection: {err}"));
            }
        }
    }
    Ok(())
}
