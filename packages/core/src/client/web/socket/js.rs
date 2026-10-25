//! The JS side of the feed: the closures the socket and the timer call, each
//! holding the driver weakly, and what is read off the global object.

use std::{cell::RefCell, rc::Weak};

use crate::client::web::clock;
use crate::client::web::socket::Driver;
use js_sys::{Array, Function, Math, Reflect};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{CloseEvent, MessageEvent, WebSocket};

/// The closures JS calls, made once a feed and dropped with it. Each holds the
/// driver weakly and is handed to every socket the feed dials in turn.
pub(super) struct Calls {
    pub(super) open: Closure<dyn FnMut()>,
    pub(super) message: Closure<dyn FnMut(MessageEvent)>,
    pub(super) close: Closure<dyn FnMut(CloseEvent)>,
    pub(super) error: Closure<dyn FnMut(JsValue)>,
    pub(super) wake: Closure<dyn FnMut()>,
}

impl Calls {
    pub(super) fn new(driver: &Weak<RefCell<Driver>>) -> Calls {
        let (open, message, close, error, wake) =
            (driver.clone(), driver.clone(), driver.clone(), driver.clone(), driver.clone());
        Calls {
            open: Closure::new(move || on(&open, Driver::opened)),
            message: Closure::new(move |event: MessageEvent| on(&message, |driver| driver.received(&event))),
            close: Closure::new(move |event: CloseEvent| {
                let reason = match event.reason() {
                    reason if reason.is_empty() => format!("the socket closed ({})", event.code()),
                    reason => reason,
                };
                on(&close, |driver| driver.lost(reason));
            }),
            // An error always ends the socket; the close that follows is not
            // waited for.
            error: Closure::new(move |event: JsValue| {
                let reason = said(&event).unwrap_or_else(|| "the socket failed".to_owned());
                on(&error, |driver| driver.lost(reason));
            }),
            wake: Closure::new(move || on(&wake, Driver::woke)),
        }
    }
}

/// Runs `act` on the driver for JS, then wakes `next` if it has something. The
/// waker is called once the driver is let go, so one that polls at once finds
/// it free.
fn on(driver: &Weak<RefCell<Driver>>, act: impl FnOnce(&mut Driver)) {
    let Some(driver) = driver.upgrade() else { return };
    let waker = {
        let Ok(mut driver) = driver.try_borrow_mut() else { return };
        act(&mut driver);
        driver.wakeable()
    };
    if let Some(waker) = waker {
        waker.wake();
    }
}

/// A new socket to `url`, from the global `WebSocket`.
pub(super) fn connect(global: &JsValue, url: &str) -> Result<WebSocket, String> {
    let class = function(global, "WebSocket").ok_or_else(|| "this runtime has no WebSocket".to_owned())?;
    let socket = Reflect::construct(&class, &Array::of1(&JsValue::from_str(url)))
        .map_err(|error| said(&error).unwrap_or_else(|| "the socket could not be made".to_owned()))?;
    Ok(socket.unchecked_into())
}

/// The global function `name`, if the runtime has it.
pub(super) fn function(global: &JsValue, name: &str) -> Option<Function> {
    Reflect::get(global, &JsValue::from_str(name)).ok()?.dyn_into().ok()
}

/// The `message` an error or an error event carries, if any. A browser's
/// socket error carries none.
pub(super) fn said(error: &JsValue) -> Option<String> {
    let message = Reflect::get(error, &JsValue::from_str("message")).ok()?.as_string()?;
    (!message.is_empty()).then_some(message)
}

/// Enough to spread reconnects: many clients dropped at once must not return
/// at once. Not for anything secret. `Math.random()`'s bits, mixed so the low
/// ones the session's jitter reads are not a small float's trailing zeros.
pub(super) fn random() -> u64 {
    let bits = Math::random().to_bits().wrapping_mul(0x9e37_79b9_7f4a_7c15);
    bits ^ (bits >> 29) ^ clock().rotate_left(29)
}
