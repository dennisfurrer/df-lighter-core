//! The websocket for native programs: a [`Feed`] carries one [`crate::live::Session`] over
//! tokio-tungstenite and hands back [`crate::live::Event`]s, each frame's text beside it when
//! asked. Subscriptions made while down are sent on the next open; what the session owes the venue
//! (a pong, a book's resubscribe after a gap) goes out as soon as it is owed; a transaction made
//! while down is refused, and one cut off by a drop is reported, never resent.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::client::native::clock;
use crate::live::{Event, FeedError, PING, Refused, Session, Subscription, Tick};
use crate::utils::sign::Signed;
use futures_util::{SinkExt, StreamExt};
use tokio::{
    net::TcpStream,
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{Instant, sleep_until},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, Utf8Bytes},
};

enum Command {
    Subscribe(Subscription, oneshot::Sender<Result<(), Refused>>),
    Unsubscribe(Subscription),
    Resubscribe(Subscription),
    Auth(Option<String>),
    SendTx(u8, String, oneshot::Sender<Result<u64, Refused>>),
    SendBatch(Vec<(u8, String)>, oneshot::Sender<Result<u64, Refused>>),
}

/// An event, and for a frame the text it arrived as.
type Delivered = (Event, Option<Utf8Bytes>);

/// One websocket connection, kept open. Dropping the feed closes it.
pub struct Feed {
    commands: mpsc::UnboundedSender<Command>,
    events: mpsc::UnboundedReceiver<Delivered>,
    task: JoinHandle<()>,
}

impl Feed {
    /// Connects to the session's socket (its network's, or the read-only one) on the current
    /// tokio runtime.
    #[must_use]
    pub fn start(session: Session) -> Feed {
        let url = session.url();
        Feed::start_at(url, session)
    }

    /// Connects to `url` instead of the session's network: a proxy, or a test.
    #[must_use]
    pub fn start_at(url: impl Into<String>, session: Session) -> Feed {
        let (commands, inbox) = mpsc::unbounded_channel();
        let (outbox, events) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(url.into(), session, inbox, outbox));
        Feed { commands, events, task }
    }

    /// Wants `subscription` from now on, across reconnects. Refused past the session's limits,
    /// or without the token it needs.
    pub async fn subscribe(&self, subscription: Subscription) -> Result<(), FeedError> {
        let (reply, answer) = oneshot::channel();
        self.commands.send(Command::Subscribe(subscription, reply)).map_err(|_| FeedError::Stopped)?;
        Ok(answer.await.map_err(|_| FeedError::Stopped)??)
    }

    pub fn unsubscribe(&self, subscription: Subscription) -> Result<(), FeedError> {
        self.commands.send(Command::Unsubscribe(subscription)).map_err(|_| FeedError::Stopped)
    }

    /// Asks for a fresh snapshot of a channel already wanted. A book's gap needs none of this:
    /// the session resubscribes it on its own.
    pub fn resubscribe(&self, subscription: Subscription) -> Result<(), FeedError> {
        self.commands.send(Command::Resubscribe(subscription)).map_err(|_| FeedError::Stopped)
    }

    /// A fresh auth token ([`crate::Client::auth_token`]), or none, for the next subscribe,
    /// resubscribe or reconnect.
    pub fn set_auth(&self, token: Option<String>) -> Result<(), FeedError> {
        self.commands.send(Command::Auth(token)).map_err(|_| FeedError::Stopped)
    }

    /// Sends a signed transaction over the socket and gives its id; the answer arrives as
    /// [`crate::live::Frame::Sent`] or [`crate::live::Frame::Error`] naming the same id.
    pub async fn send_tx(&self, signed: &Signed) -> Result<u64, FeedError> {
        self.send_tx_info(signed.tx_type().code(), signed.tx_info()).await
    }

    /// [`Feed::send_tx`] for a type and `tx_info` as signed: one with the owner's signature in it
    /// ([`Signed::tx_info_with_l1`]).
    pub async fn send_tx_info(&self, tx_type: u8, tx_info: String) -> Result<u64, FeedError> {
        let (reply, answer) = oneshot::channel();
        self.commands.send(Command::SendTx(tx_type, tx_info, reply)).map_err(|_| FeedError::Stopped)?;
        Ok(answer.await.map_err(|_| FeedError::Stopped)??)
    }

    /// Sends up to fifty signed transactions as one batch and gives its id.
    pub async fn send_tx_batch(&self, signed: &[Signed]) -> Result<u64, FeedError> {
        let txs = signed.iter().map(|one| (one.tx_type().code(), one.tx_info())).collect();
        let (reply, answer) = oneshot::channel();
        self.commands.send(Command::SendBatch(txs, reply)).map_err(|_| FeedError::Stopped)?;
        Ok(answer.await.map_err(|_| FeedError::Stopped)??)
    }

    /// The next event; `None` once the feed has stopped.
    pub async fn next(&mut self) -> Option<Event> {
        self.events.recv().await.map(|(event, _)| event)
    }

    /// The next event, and for a frame, read or not, the text it arrived as: for a program that
    /// still reads some channels its own way. `None` once the feed has stopped.
    pub async fn next_with_text(&mut self) -> Option<(Event, Option<String>)> {
        let (event, text) = self.events.recv().await?;
        Some((event, text.map(|text| text.as_str().to_owned())))
    }

    /// Closes the socket and stops.
    pub fn close(self) {
        drop(self);
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        self.task.abort();
    }
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Whether the connection was lost, and why, or the feed is done.
enum Outcome {
    Lost(String),
    Stop,
}

async fn run(
    url: String,
    mut session: Session,
    mut commands: mpsc::UnboundedReceiver<Command>,
    events: mpsc::UnboundedSender<Delivered>,
) {
    loop {
        // While it dials, the session still answers: a subscription is kept for the open, a
        // transaction is refused. Nothing waits on a handshake.
        let dial = connect_async(url.as_str());
        tokio::pin!(dial);
        let dialled = loop {
            tokio::select! {
                dialled = &mut dial => break dialled,
                command = commands.recv() => match command {
                    Some(command) => {
                        handle(&mut session, command);
                    }
                    None => return,
                },
            }
        };
        let reason = match dialled {
            Ok((mut socket, _)) => match open(&mut socket, &mut session, &mut commands, &events).await {
                Outcome::Lost(reason) => reason,
                Outcome::Stop => return,
            },
            Err(error) => error.to_string(),
        };
        let retry = session.closed(clock(), random());
        let until = Instant::now() + Duration::from_millis(retry.after_ms);
        if events.send((Event::Closed { reason, retry }, None)).is_err() {
            return;
        }
        // Until the next dial, subscriptions are kept for it and transactions refused.
        loop {
            tokio::select! {
                () = sleep_until(until) => break,
                command = commands.recv() => match command {
                    Some(command) => {
                        handle(&mut session, command);
                    }
                    None => return,
                },
            }
        }
    }
}

async fn open(
    socket: &mut Socket,
    session: &mut Session,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    events: &mpsc::UnboundedSender<Delivered>,
) -> Outcome {
    if let Err(reason) = send(socket, session.opened(clock())).await {
        return Outcome::Lost(reason);
    }
    if events.send((Event::Opened, None)).is_err() {
        return Outcome::Stop;
    }
    loop {
        let wake = session.due().map(|at| Instant::now() + Duration::from_millis(at.saturating_sub(clock())));
        tokio::select! {
            message = socket.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    let event = match session.received(clock(), &text) {
                        Ok(frame) => Event::Frame(Box::new(frame)),
                        Err(bad) => Event::Unreadable(bad),
                    };
                    if let Err(reason) = send(socket, session.owed()).await {
                        return Outcome::Lost(reason);
                    }
                    if events.send((event, Some(text))).is_err() {
                        return Outcome::Stop;
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    return Outcome::Lost(frame.map_or_else(|| "the venue closed the socket".to_owned(), |frame| frame.reason.to_string()));
                }
                // Protocol pings are answered by tungstenite; the venue's own heartbeat is text.
                Some(Ok(_)) => {}
                Some(Err(error)) => return Outcome::Lost(error.to_string()),
                None => return Outcome::Lost("the socket ended".to_owned()),
            },
            command = commands.recv() => match command {
                Some(command) => {
                    if let Err(reason) = send(socket, handle(session, command)).await {
                        return Outcome::Lost(reason);
                    }
                }
                None => {
                    let _ = socket.close(None).await;
                    return Outcome::Stop;
                }
            },
            () = until(wake) => match session.tick(clock()) {
                Tick::Wait => {}
                Tick::Ping => {
                    if let Err(error) = socket.send(Message::text(PING)).await {
                        return Outcome::Lost(error.to_string());
                    }
                }
                Tick::Reconnect => {
                    let _ = socket.close(None).await;
                    return Outcome::Lost("nothing arrived for too long".to_owned());
                }
            },
        }
    }
}

/// Sends `frames` in order, or says why the socket would not take them.
async fn send(socket: &mut Socket, frames: Vec<String>) -> Result<(), String> {
    for frame in frames {
        socket.send(Message::text(frame)).await.map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Puts a command to the session and answers its caller; gives the frames to send, if the socket
/// is open and there are any.
fn handle(session: &mut Session, command: Command) -> Vec<String> {
    match command {
        Command::Subscribe(subscription, reply) => match session.subscribe(subscription) {
            Ok(frame) => {
                let _ = reply.send(Ok(()));
                frame.into_iter().collect()
            }
            Err(refused) => {
                let _ = reply.send(Err(refused));
                Vec::new()
            }
        },
        Command::Unsubscribe(subscription) => session.unsubscribe(&subscription).into_iter().collect(),
        Command::Resubscribe(subscription) => session.resubscribe(&subscription),
        Command::Auth(token) => {
            session.set_auth(token);
            Vec::new()
        }
        Command::SendTx(tx_type, tx_info, reply) => answer(session.send_tx_info(tx_type, &tx_info), reply),
        Command::SendBatch(txs, reply) => answer(session.send_tx_batch_infos(&txs), reply),
    }
}

/// A transaction's id to its caller, and its frame to send.
fn answer(sent: Result<(u64, String), Refused>, reply: oneshot::Sender<Result<u64, Refused>>) -> Vec<String> {
    match sent {
        Ok((id, frame)) => {
            let _ = reply.send(Ok(id));
            vec![frame]
        }
        Err(refused) => {
            let _ = reply.send(Err(refused));
            Vec::new()
        }
    }
}

async fn until(wake: Option<Instant>) {
    match wake {
        Some(at) => sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// Enough to spread reconnects: many clients dropped at once must not return at once. Not for
/// anything secret.
fn random() -> u64 {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| u64::from(since.subsec_nanos()));
    nanos.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ clock().rotate_left(29)
}

#[cfg(test)]
#[path = "tests/socket.rs"]
mod tests;
