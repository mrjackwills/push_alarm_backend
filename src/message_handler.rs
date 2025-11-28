use async_channel::{Receiver, Sender};
use sqlx::SqlitePool;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;

use crate::{
    C,
    alarm_schedule::AlarmSchedule,
    app_env::AppEnv,
    request::PushRequest,
    sleep,
    ws::{ConnectionDetails, Socket, WSSender, open_connection},
    ws_messages::Response,
};

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub type WSReader =
    futures_util::stream::SplitStream<Box<WebSocketStream<MaybeTlsStream<TcpStream>>>>;
pub type WSWriter = futures_util::stream::SplitSink<
    Box<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    tokio_tungstenite::tungstenite::Message,
>;

const TWENTY_FIVE_SEC: std::time::Duration = std::time::Duration::from_secs(25);

#[derive(Debug)]
pub struct WSResponse {
    pub response: Response,
    pub cache: Option<bool>,
    pub unique: Option<String>,
}

#[derive(Debug)]
pub enum Msg {
    AlarmDissmiss,
    AlarmStart(Option<String>),
    AlarmLoopReset,
    Exit,
    Ping,
    Received(String),
    ToSend(WSResponse),
    WsClose,
    WsConnected(Box<WsStream>),
}

#[derive(Debug)]
pub struct MessageHandler {
    app_env: AppEnv,
    alarm_schedule: AlarmSchedule,
    alarm_token: Option<CancellationToken>,
    rx: Receiver<Msg>,
    connection_details: ConnectionDetails,
    socket: Option<Socket>,
    tx: Sender<Msg>,
    sqlite: SqlitePool,
    ws_sender: WSSender,
}

impl MessageHandler {
    /// Send a status update, will be spawned in own thread before sending back to message handler here
    fn send_status(&self, ms: Option<u64>) {
        let ws = C!(self.ws_sender);
        tokio::spawn(async move {
            if let Some(ms) = ms {
                sleep!(ms);
            }
            ws.send_status().await;
        });
    }

    /// Spawn a loop to send messages, not over WS, every X seconds
    fn start_alarm_requests(&mut self, request_msg: Option<String>) {
        let app_envs = C!(self.app_env);
        let sqlite = C!(self.sqlite);
        if let Some(token) = &self.alarm_token {
            token.cancel();
        }

        let token = CancellationToken::new();
        self.alarm_token = Some(C!(token));

        tokio::spawn(async move {
            token
                .run_until_cancelled(async {
                    let msg = PushRequest::get_message(&sqlite, request_msg).await;
                    for i in 1..=40 {
                        if let Err(e) = PushRequest::Alarm(i)
                            .make_request(&app_envs, &sqlite, &msg)
                            .await
                        {
                            tracing::error!("{e}");
                        }
                        tokio::time::sleep(TWENTY_FIVE_SEC).await;
                    }
                })
                .await;
        });
        // }
    }

    /// Start the message handler
    pub async fn start(&mut self) {
        tokio::join!(
            self.alarm_schedule.start_alarm_thread(&self.sqlite),
            open_connection(&self.app_env, &self.tx, &mut self.connection_details)
        );

        while let Ok(msg) = self.rx.recv().await {
            match msg {
                Msg::Exit => {
                    if let Some(socket) = &mut self.socket {
                        socket.close().await;
                    }
                }
                Msg::Ping => {
                    if let Some(socket) = &mut self.socket {
                        socket.on_ping(&self.tx);
                    }
                }
                Msg::Received(msg) => {
                    let ws_sender = C!(self.ws_sender);
                    tokio::spawn(async move {
                        ws_sender.on_text(msg).await;
                    });
                }
                Msg::AlarmLoopReset => {
                    self.alarm_schedule.start_alarm_thread(&self.sqlite).await;
                }
                Msg::AlarmDissmiss => {
                    if let Some(token) = &self.alarm_token {
                        token.cancel();
                    }
                }
                Msg::AlarmStart(request_msg) => {
                    Self::start_alarm_requests(self, request_msg);
                }
                Msg::ToSend(ws_response) => {
                    if let Some(socket) = &mut self.socket {
                        socket
                            .send(ws_response.response, ws_response.cache, ws_response.unique)
                            .await;
                    }
                }
                Msg::WsClose => {
                    if let Some(socket) = &mut self.socket {
                        socket.close().await;
                    }
                    open_connection(&self.app_env, &self.tx, &mut self.connection_details).await;
                    self.ws_sender.on_connection();
                }
                Msg::WsConnected(stream) => {
                    self.socket = Some(Socket::new(stream, &self.tx));
                    self.send_status(None);
                }
            }
        }
    }

    pub fn new(app_env: AppEnv, sqlite: SqlitePool, rx: Receiver<Msg>, tx: Sender<Msg>) -> Self {
        let ws_sender = WSSender::new(&app_env, &sqlite, &tx);
        let alarm_schedule = AlarmSchedule::new(&tx);

        Self {
            app_env,
            alarm_token: None,
            alarm_schedule,
            connection_details: ConnectionDetails::new(),
            rx,
            socket: None,
            sqlite,
            tx,
            ws_sender,
        }
    }
}
