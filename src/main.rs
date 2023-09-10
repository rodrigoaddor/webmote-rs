use std::net::SocketAddr;

use clap::{arg, Parser};
use lazy_static::lazy_static;
use log::{error, info};
use warp::Filter;
use crate::controllers::socket::ws_connected;

mod controllers;
mod public;

#[derive(Parser)]
/// Webmote RTC server
struct Cli {
    #[arg(short, long, env)]
    server: Vec<String>,
    #[arg(short, long, env)]
    listen: String,
}

// Public config
lazy_static! {
    pub static ref SERVER_LIST: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(vec!["stun:stun.l.google.com:19302".to_owned()]));
}

pub type WarpResult<T> = ::std::result::Result<T, warp::Rejection>;

#[tokio::main]
async fn main() {
    env_logger::init();

    let cli = Cli::parse();
    if cli.server.is_empty() {
        error!("No server specified");
        return;
    }

    let mut server_list = SERVER_LIST.lock().unwrap();
    *server_list = cli.server.clone();
    drop(server_list);

    // let mut addr: SocketAddr = "[::1]:3000".parse().unwrap();
    let addr: SocketAddr = cli.listen.parse().unwrap();

    let ws = warp::path("ws")
        .and(warp::addr::remote())
        .and(warp::ws())
        .map(|addr: Option<SocketAddr>, ws: warp::ws::Ws| {
            info!("Websocket connected from {}", addr.unwrap());
            ws.on_upgrade(|socket| async { ws_connected(socket).await })
        });

    /*let index = warp::path::end()
        .and_then(|controller: Controller| async move {
            let gamepad = controller.get().await;
            WarpResult::Ok(json_data::Controller {
                user_id: 0,
                axis: json_data::Axis {
                    left_x: gamepad.thumb_lx
                },
            })
        })
        .map(|value: json_data::Controller| warp::reply::json(&value));

    let routes = index.or(ws);*/
    let routes = ws;

    info!("Listening on: {}", addr);
    warp::serve(routes).run(addr).await;
}
