use tokio::{io::AsyncReadExt, net::TcpListener};
use tokio_util::bytes::BytesMut;

#[tokio::main]
async fn main() {
    // Install logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("trace")).init();

    // Start server
    let server = TcpListener::bind("127.0.0.1:8080").await.unwrap();

    while let Ok((mut stream, addr)) = server.accept().await {
        tokio::spawn(async move {
            log::info!("{addr:?}: connected!");
            loop {
                let mut buf = BytesMut::new();
                let Ok(bytes) = stream.read_buf(&mut buf).await else {
                    break;
                };
                if bytes == 0 {
                    break;
                }

                let bytes = &buf[0..bytes];
                log::info!("{addr:?}: {bytes:?}");
            }

            log::info!("{addr:?}: disconnected!");
        });
    }
}
