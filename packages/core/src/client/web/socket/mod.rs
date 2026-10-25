//! The websocket in the browser and Workers: a [`Feed`] carries one [`crate::live::Session`]
//! over the global `WebSocket` and hands back [`crate::live::Event`]s, each frame's text beside
//! it when asked. Subscriptions made while down are sent on the next open; what the session owes
//! the venue (a pong, a book's resubscribe after a gap) goes out as soon as it is owed; a
//! transaction made while down is refused, and one cut off by a drop is reported, never resent.
//!
//! Everything runs on the one JS thread: the socket's events and the timer call into the
//! feed as tasks of their own, and [`Feed::next`] waits for the events they leave.

mod js;

use std::{
    cell::RefCell,
    collections::VecDeque,
    future::poll_fn,
    rc::Rc,
    task::{Poll, Waker},
};

use crate::client::web::clock;
use crate::client::web::socket::js::{Calls, connect, function, random, said};
use crate::live::{Event, FeedError, PING, Session, Subscription, Tick};
use crate::utils::sign::Signed;
use js_sys::Function;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{MessageEvent, WebSocket};

/// One websocket connection, kept open. Dropping the feed closes it.
pub struct Feed {
    driver: Rc<RefCell<Driver>>,
}

impl Feed {
    /// Connects to the session's socket (its network's, or the read-only one).
    #[must_use]
    pub fn start(session: Session) -> Feed {
        let url = session.url();
        Feed::start_at(url, session)
    }

    /// Connects to `url` instead of the session's network: a proxy, or a test.
    #[must_use]
    pub fn start_at(url: impl Into<String>, session: Session) -> Feed {
        let global: JsValue = js_sys::global().into();
        let timers = function(&global, "setTimeout").zip(function(&global, "clearTimeout"));
        let driver = Rc::new_cyclic(|driver| {
            RefCell::new(Driver {
                url: url.into(),
                session,
                global,
                timers,
                socket: None,
                timer: None,
                calls: Calls::new(driver),
                events: VecDeque::new(),
                waker: None,
                stopped: false,
            })
        });
        if let Ok(mut driver) = driver.try_borrow_mut() {
            if driver.timers.is_some() {
                driver.dial();
            } else {
                driver.stop();
            }
        }
        Feed { driver }
    }

    /// Wants `subscription` from now on, across reconnects. Refused past the session's limits,
    /// or without the token it needs.
    pub async fn subscribe(&self, subscription: Subscription) -> Result<(), FeedError> {
        self.with(|driver| {
            if let Some(frame) = driver.session.subscribe(subscription)? {
                driver.carry(&frame);
            }
            Ok(())
        })
    }

    pub fn unsubscribe(&self, subscription: Subscription) -> Result<(), FeedError> {
        self.with(|driver| {
            if let Some(frame) = driver.session.unsubscribe(&subscription) {
                driver.carry(&frame);
            }
            Ok(())
        })
    }

    /// Asks for a fresh snapshot of a channel already wanted. A book's gap needs none of this:
    /// the session resubscribes it on its own.
    pub fn resubscribe(&self, subscription: Subscription) -> Result<(), FeedError> {
        self.with(|driver| {
            for frame in driver.session.resubscribe(&subscription) {
                driver.carry(&frame);
            }
            Ok(())
        })
    }

    /// A fresh auth token ([`crate::Client::auth_token`]), or none, for the next subscribe,
    /// resubscribe or reconnect.
    pub fn set_auth(&self, token: Option<String>) -> Result<(), FeedError> {
        self.with(|driver| {
            driver.session.set_auth(token);
            Ok(())
        })
    }

    /// Sends a signed transaction over the socket and gives its id; the answer arrives as
    /// [`crate::live::Frame::Sent`] or [`crate::live::Frame::Error`] naming the same id.
    pub async fn send_tx(&self, signed: &Signed) -> Result<u64, FeedError> {
        self.send_tx_info(signed.tx_type().code(), signed.tx_info()).await
    }

    /// [`Feed::send_tx`] for a type and `tx_info` as signed: one with the owner's signature in it
    /// ([`Signed::tx_info_with_l1`]).
    pub async fn send_tx_info(&self, tx_type: u8, tx_info: String) -> Result<u64, FeedError> {
        self.with(|driver| {
            let (id, frame) = driver.session.send_tx_info(tx_type, &tx_info)?;
            driver.carry(&frame);
            Ok(id)
        })
    }

    /// Sends up to fifty signed transactions as one batch and gives its id.
    pub async fn send_tx_batch(&self, signed: &[Signed]) -> Result<u64, FeedError> {
        self.with(|driver| {
            let (id, frame) = driver.session.send_tx_batch(signed)?;
            driver.carry(&frame);
            Ok(id)
        })
    }

    /// The next event; `None` once the feed has stopped.
    pub async fn next(&mut self) -> Option<Event> {
        self.next_with_text().await.map(|(event, _)| event)
    }

    /// The next event, and for a frame, read or not, the text it arrived as: for a program that
    /// still reads some channels its own way. `None` once the feed has stopped.
    pub async fn next_with_text(&mut self) -> Option<(Event, Option<String>)> {
        poll_fn(|cx| {
            let Ok(mut driver) = self.driver.try_borrow_mut() else {
                // Never: the driver is only held inside a call, never across one.
                cx.waker().wake_by_ref();
                return Poll::Pending;
            };
            if let Some(delivered) = driver.events.pop_front() {
                return Poll::Ready(Some(delivered));
            }
            if driver.stopped {
                return Poll::Ready(None);
            }
            driver.waker = Some(cx.waker().clone());
            Poll::Pending
        })
        .await
    }

    /// Closes the socket and stops.
    pub fn close(self) {
        drop(self);
    }

    /// Runs `act` on a feed that has not stopped. An event it leaves needs no
    /// wake: [`Feed::next`] takes the feed mutably, so nothing waits on it.
    fn with<T>(&self, act: impl FnOnce(&mut Driver) -> Result<T, FeedError>) -> Result<T, FeedError> {
        let mut driver = self.driver.try_borrow_mut().map_err(|_| FeedError::Stopped)?;
        if driver.stopped {
            return Err(FeedError::Stopped);
        }
        act(&mut driver)
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        if let Ok(mut driver) = self.driver.try_borrow_mut() {
            driver.stop();
        }
    }
}

/// The feed's state. The socket's handlers and the timer hold it weakly, so
/// the feed alone keeps it, and with it every closure JS can call.
struct Driver {
    url: String,
    session: Session,
    global: JsValue,
    /// The global `setTimeout` and `clearTimeout`.
    timers: Option<(Function, Function)>,
    /// The socket dialling or open; none while waiting to dial again.
    socket: Option<WebSocket>,
    /// The pending timeout: the next tick while open, the next dial while
    /// closed, none while dialling. A browser gives a number, Node an object.
    timer: Option<JsValue>,
    calls: Calls,
    /// Each event, and for a frame the text it arrived as.
    events: VecDeque<(Event, Option<String>)>,
    waker: Option<Waker>,
    stopped: bool,
}

impl Driver {
    /// A new socket. While it dials, the session still answers: a subscription
    /// is kept for the open, a transaction is refused. Nothing waits on a handshake.
    fn dial(&mut self) {
        match connect(&self.global, &self.url) {
            Ok(socket) => {
                socket.set_onopen(Some(self.calls.open.as_ref().unchecked_ref()));
                socket.set_onmessage(Some(self.calls.message.as_ref().unchecked_ref()));
                socket.set_onclose(Some(self.calls.close.as_ref().unchecked_ref()));
                socket.set_onerror(Some(self.calls.error.as_ref().unchecked_ref()));
                self.socket = Some(socket);
            }
            Err(reason) => self.lost(reason),
        }
    }

    fn opened(&mut self) {
        for frame in self.session.opened(clock()) {
            if let Err(reason) = self.send(&frame) {
                return self.lost(reason);
            }
        }
        self.events.push_back((Event::Opened, None));
        self.arm_tick();
    }

    fn received(&mut self, message: &MessageEvent) {
        // The venue sends text only; anything else is not its.
        let Some(text) = message.data().as_string() else { return };
        let event = match self.session.received(clock(), &text) {
            Ok(frame) => Event::Frame(Box::new(frame)),
            Err(bad) => Event::Unreadable(bad),
        };
        for frame in self.session.owed() {
            if let Err(reason) = self.send(&frame) {
                self.events.push_back((event, Some(text)));
                return self.lost(reason);
            }
        }
        self.events.push_back((event, Some(text)));
    }

    /// The timer fired: a tick while open, the next dial while closed.
    fn woke(&mut self) {
        self.timer = None;
        if self.session.is_open() {
            match self.session.tick(clock()) {
                Tick::Wait => {}
                Tick::Ping => {
                    if let Err(reason) = self.send(PING) {
                        return self.lost(reason);
                    }
                }
                Tick::Reconnect => return self.lost("nothing arrived for too long".to_owned()),
            }
            self.arm_tick();
        } else if self.socket.is_none() {
            self.dial();
        }
    }

    /// Sends `frame`, or drops a socket that will not take it.
    fn carry(&mut self, frame: &str) {
        if let Err(reason) = self.send(frame) {
            self.lost(reason);
        }
    }

    fn send(&self, frame: &str) -> Result<(), String> {
        let Some(socket) = &self.socket else { return Err("no socket to send on".to_owned()) };
        socket.send_with_str(frame).map_err(|error| said(&error).unwrap_or_else(|| "the send failed".to_owned()))
    }

    /// The socket is gone, or never came: report it with the session's delay,
    /// and dial again after it.
    fn lost(&mut self, reason: String) {
        self.hang_up();
        let retry = self.session.closed(clock(), random());
        let after = retry.after_ms;
        self.events.push_back((Event::Closed { reason, retry }, None));
        self.arm(after);
    }

    /// Closes the socket after taking its handlers off, so nothing it still
    /// has queued reaches the feed. A half-open socket's close may never
    /// finish; nothing waits for it.
    fn hang_up(&mut self) {
        if let Some(socket) = self.socket.take() {
            socket.set_onopen(None);
            socket.set_onmessage(None);
            socket.set_onclose(None);
            socket.set_onerror(None);
            let _ = socket.close();
        }
    }

    fn stop(&mut self) {
        self.hang_up();
        self.disarm();
        self.stopped = true;
    }

    /// The timer for the session's next tick, while open.
    fn arm_tick(&mut self) {
        if let Some(at) = self.session.due() {
            self.arm(at.saturating_sub(clock()));
        }
    }

    /// One timeout for `ms`, in place of any pending. Without a timer the feed
    /// can neither tell a dead socket nor dial again, so it stops.
    fn arm(&mut self, ms: u64) {
        self.disarm();
        let Some((set_timeout, _)) = &self.timers else { return self.stop() };
        let delay = JsValue::from(u32::try_from(ms).map_or(LONGEST_DELAY, |ms| ms.min(LONGEST_DELAY)));
        match set_timeout.call2(&self.global, self.calls.wake.as_ref(), &delay) {
            Ok(timer) => self.timer = Some(timer),
            Err(_) => self.stop(),
        }
    }

    fn disarm(&mut self) {
        if let (Some(timer), Some((_, clear_timeout))) = (self.timer.take(), &self.timers) {
            let _ = clear_timeout.call1(&self.global, &timer);
        }
    }

    /// The waker to call once the driver is let go, when `next` has something.
    fn wakeable(&mut self) -> Option<Waker> {
        if self.events.is_empty() && !self.stopped { None } else { self.waker.take() }
    }
}

/// The longest delay `setTimeout` keeps; a longer one fires at once.
const LONGEST_DELAY: u32 = 0x7fff_ffff;

#[cfg(test)]
#[cfg(target_arch = "wasm32")]
#[path = "tests/feed.rs"]
mod tests;
