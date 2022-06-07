
#![deny(warnings)]

use std::io::{self, Write};
use std::net::TcpStream;

// use futures_util::TryStreamExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};

/// This is our service handler. It receives a Request, routes on its
/// path, and returns a Future of a Response.
async fn echo(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        // Convert to uppercase before sending back to client using a stream.
        // (&Method::POST, "/") => {
        //     let chunk_stream = req.into_body().map_ok(|chunk| {
        //         chunk
        //             .iter()
        //             .map(|byte| byte.to_ascii_uppercase())
        //             .collect::<Vec<u8>>()
        //     });
        //     Ok(Response::new(Body::wrap_stream(chunk_stream)))
        // }

        // Reverse the entire body before sending back to the client.
        //
        // Since we don't know the end yet, we can't simply stream
        // the chunks as they arrive as we did with the above uppercase endpoint.
        // So here we do `.await` on the future, waiting on concatenating the full body,
        // then afterwards the content can be reversed. Only then can we return a `Response`.
        (&Method::POST, "/") => {
            let whole_body = hyper::body::to_bytes(req.into_body()).await?;
            let body_vec = whole_body.iter().cloned().collect::<Vec<u8>>();
            tcp_send(&body_vec).unwrap();
            
            // println!("{:?}", body_str);

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


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {


    let addr = ([127, 0, 0, 1], 8000).into();

    let service = make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(echo)) });

    let server = Server::bind(&addr).serve(service);

    println!("Listening on http://{}", addr);

    server.await?;

    Ok(())
}


fn tcp_send(data: &Vec<u8>) -> io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:10032")?;
    // let mut input = String::new();
    stream.write(data).expect("failed to write");
    Ok(())
}