#![deny(warnings)]

use std::io::{self, Write};
use std::net::TcpStream;

use crossbeam_channel::{unbounded, Receiver, Sender};
use std::thread;
use std::time::Duration;
use urlparse::parse_qs;

use clap::{App, Arg, ArgMatches};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};

use byteorder::{BigEndian, WriteBytesExt};
use chrono::prelude::*;
use daemonize::Daemonize;
use std::fs::File;

use lazy_static::lazy_static;

lazy_static! {
    static ref TRANSFER_DATA_CHANNEL: (Sender<Vec<u8>>, Receiver<Vec<u8>>) = unbounded();
}

/// This is our service handler. It receives a Request, routes on its
/// path, and returns a Future of a Response.
async fn echo(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::POST, "/") => {
            let whole_body = hyper::body::to_bytes(req.into_body()).await?;
            let body_vec = whole_body.iter().cloned().collect::<Vec<u8>>();
            let data = package_build(body_vec);
            TRANSFER_DATA_CHANNEL.0.send(data).unwrap();
            Ok(Response::new(Body::from("ok")))
        }

        // Return the 404 Not Found for other routes.
        _ => {
            let mut not_found = Response::default();
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

fn parse_args() -> ArgMatches<'static> {
    let matches = App::new("datalogts")
        .help_message("")
        .version("1.0")
        .author("liu.weihua@rytong.com")
        .arg(
            Arg::with_name("http_port")
                .short("hp")
                .long("hport")
                .help("Sets a custom config file")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("tcp_host")
                .short("th")
                .long("thost")
                .help("Sets a custom config file")
                .takes_value(true),
        )
        .get_matches();
    matches
}

async fn http_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let matches = parse_args();

    let http_port = matches.value_of("http_port").unwrap_or("8000");
    let tcp_host = matches.value_of("tcp_host").unwrap_or("127.0.0.1:8001");

    let http_port = String::from(http_port).parse::<u16>().unwrap();

    tcp_send(&tcp_host)?;

    let addr = ([0, 0, 0, 0], http_port).into();
    let service = make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(echo)) });
    let server = Server::bind(&addr).serve(service);
    server.await?;
    Ok(())
}

fn main() {
    let stdout = File::create("datalogts.out").unwrap();
    let stderr = File::create("datalogts.err").unwrap();

    let daemonize = Daemonize::new()
        .pid_file("datalogts.pid") // Every method except `new` and `start`
        .chown_pid_file(true) // is optional, see `Daemonize` documentation
        .working_directory(".") // for default behaviour.
        .user("nobody")
        .group("daemon") // Group name
        // .group(2) // or group id.
        .umask(0o777) // Set umask, `0o027` by default.
        .chroot(".")
        .stdout(stdout) // Redirect stdout to `/tmp/daemon.out`.
        .stderr(stderr) // Redirect stderr to `/tmp/daemon.err`.
        .exit_action(|| {
            println!("Executed before master process exits");
        })
        .privileged_action(|| "Executed before drop privileges");

    match daemonize.start() {
        Ok(_) => println!("Success, daemonized"),
        Err(e) => eprintln!("Error, {}", e),
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            http_server().await.unwrap();
        })
}

fn tcp_send(host: &str) -> io::Result<()> {
    let mut stream = TcpStream::connect(host)?;

    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(120));
        let data = TRANSFER_DATA_CHANNEL.1.recv().unwrap();
        if !data.is_empty() {
            stream.write(&data).expect("failed to write");
        }
    });

    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(3));
        let data: Vec<u8> = vec![0xf0, 0x0, 0x0, 0x0, 0x7, 0x0, 0xfe];
        TRANSFER_DATA_CHANNEL.0.send(data).unwrap();
    });

    Ok(())
}

/**
 * package build transer
 */
fn package_build(data: Vec<u8>) -> Vec<u8> {
    let str = String::from_utf8(data).expect("Found invalid UTF-8");
    let map = parse_qs(str.as_str());
    let api = map.get(&"api".to_string()).unwrap();
    let app = map.get(&"app".to_string()).unwrap();
    let env = map.get(&"env".to_string()).unwrap();
    let content = map.get(&"data".to_string()).unwrap();
    let content_slice = content.get(0).unwrap().as_bytes();

    let len = 1 + 4 + 1 + 1 + 3 * 12 + 8 + content_slice.len() + 1;
    let mut offset: usize = 0;
    let mut res: Vec<u8> = vec![0; len];
    res[offset] = 0xf0;
    offset += 1;
    let mut len_vec = vec![];
    len_vec.write_u32::<BigEndian>(len as u32).unwrap();
    res[offset..offset + 4].copy_from_slice(len_vec.as_slice());
    offset += 4;
    res[offset] = 0x01;
    offset += 1;
    res[offset] = 0x03;
    offset += 1;
    let app_slice = app.get(0).unwrap().as_bytes();
    res[offset..offset + app_slice.len()].copy_from_slice(app_slice);
    offset += 12;
    let env_slice = env.get(0).unwrap().as_bytes();
    res[offset..offset + env_slice.len()].copy_from_slice(env_slice);
    offset += 12;
    let api_slice = api.get(0).unwrap().as_bytes();
    res[offset..offset + api_slice.len()].copy_from_slice(api_slice);
    offset += 12;
    let datestr = Utc::today().format("%Y%m%d");
    res[offset..offset + 8].copy_from_slice(datestr.to_string().as_bytes());
    offset += 8;
    res[offset..offset + content_slice.len()].copy_from_slice(content_slice);
    offset += content_slice.len();
    res[offset] = 0xfe;
    res
}
