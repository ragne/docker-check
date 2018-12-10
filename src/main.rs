use std::io;
use std::io::prelude::*;
use std::net::{Shutdown, TcpListener, TcpStream};

mod threadpool;
extern crate dockworker;

use dockworker::{Docker, container::ContainerFilters, ContainerListOptions};

fn handle_client(mut stream: TcpStream) {
    // ...
    let mut should_stop = false;
    let response = format!(
        "HTTP/1.1 200 OK{ending}\
         Content-Type: text/plain{ending}\
         Content-Length: 0{ending}{ending}",
        ending = "\r\n"
    );
    loop {
        let mut buf = vec![0; 512];
        match stream.read(&mut buf) {
            Ok(read_bytes) => {
                if read_bytes == 0 {
                    should_stop = true
                }
                println!("read {} bytes!", read_bytes);
                println!(
                    "trying to decode: {:?}\n",
                    String::from_utf8_lossy(&buf[0..read_bytes])
                );
                match stream.write(response.as_bytes()) {
                    Ok(bytes_written) => println!("wrote {} bytes!", bytes_written),
                    Err(ref e) => match e.kind() {
                        std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::BrokenPipe => {
                            should_stop = true
                        }
                        x => {
                            println!("stream.read::Error: {:?}", x);
                            should_stop = true
                        }
                    },
                };
                stream.flush().unwrap();
            }
            Err(ref e) => match e.kind() {
                std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::BrokenPipe => {
                    should_stop = true
                }
                x => {
                    println!("stream.read::Error: {:?}", x);
                    should_stop = true
                }
            },
        };

        println!("there!!!");
        if should_stop {
            println!("EOF reached!");
            stream.shutdown(Shutdown::Both).unwrap_or(());
            break;
        }
    }
}

use std::thread;
use std::time::Duration;

fn listen() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8000")?;
    let pool = threadpool::ThreadPool::new(5);
    // accept connections and process them serially
    thread::spawn(move || {
        for stream in listener.incoming() {
            pool.execute(|| {
                handle_client(stream.unwrap());
            });
        }
    });
    let docker = Docker::connect_with_http("http://localhost:2376").unwrap();
    

    let filter = ContainerFilters::new();
    let containers = docker.list_containers(None, None, None, filter).unwrap();

    loop {
            containers.iter().for_each(|c| {
                let res = docker.container_info(&c).unwrap();
                    println!("{:?}", res.State.Health.Status);
            });
        thread::sleep(Duration::from_secs(10));


    }
    Ok(())
}

fn main() {
    println!("Hello, world!");
    listen().unwrap();
}
