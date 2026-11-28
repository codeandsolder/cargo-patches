//! Smithay Clipboard
//!
//! Provides access to the Wayland clipboard for gui applications. The user
//! should have surface around.

#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use)]
use std::ffi::c_void;
use std::io::{Error, Result};
use std::sync::mpsc::{self, Receiver};

use sctk::reexports::calloop::channel::{self, Sender};
use sctk::reexports::client::backend::Backend;
use sctk::reexports::client::Connection;

mod mime;
mod state;
mod worker;

/// Access to a Wayland clipboard.
pub struct Clipboard {
    request_sender: Sender<worker::Command>,
    request_receiver: Receiver<Result<String>>,
    clipboard_thread: Option<std::thread::JoinHandle<()>>,
}

impl Clipboard {
    /// Creates new clipboard which will be running on its own thread with its
    /// own event queue to handle clipboard requests.
    ///
    /// # Safety
    ///
    /// `display` must be a valid `*mut wl_display` pointer, and it must remain
    /// valid for as long as `Clipboard` object is alive.
    pub unsafe fn new(display: *mut c_void) -> Result<Self> {
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        Self::from_connection(Connection::from_backend(backend))
    }

    /// Create a clipboard on an owned Wayland connection, with an independent
    /// event queue. A successful result means the clipboard worker has
    /// initialized.
    pub fn from_connection(connection: Connection) -> Result<Self> {
        let (request_sender, rx_chan) = channel::channel();
        let (clipboard_reply_sender, request_receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let clipboard_thread = worker::spawn(
            String::from("smithay-clipboard"),
            connection,
            rx_chan,
            clipboard_reply_sender,
            ready_sender,
        )?;
        let ready = ready_receiver
            .recv()
            .map_err(|_| Error::other("clipboard worker exited during startup"))?;
        if let Err(error) = ready {
            let _ = clipboard_thread.join();
            return Err(error);
        }
        Ok(Self { request_receiver, request_sender, clipboard_thread: Some(clipboard_thread) })
    }

    /// Load clipboard data.
    ///
    /// Loads content from a clipboard on a last observed seat.
    pub fn load(&self) -> Result<String> {
        self.request_sender
            .send(worker::Command::Load)
            .map_err(|_| Error::other("clipboard worker is dead"))?;

        if let Ok(reply) = self.request_receiver.recv() {
            reply
        } else {
            // The clipboard thread is dead, however we shouldn't crash
            // downstream, so propogating an error.
            Err(std::io::Error::other("clipboard is dead."))
        }
    }

    /// Store to a clipboard.
    ///
    /// Stores to a clipboard on a last observed seat.
    pub fn store<T: Into<String>>(&self, text: T) -> Result<()> {
        self.store_selection(text.into(), false)
    }

    /// Load primary clipboard data.
    ///
    /// Loads content from a  primary clipboard on a last observed seat.
    pub fn load_primary(&self) -> Result<String> {
        self.request_sender
            .send(worker::Command::LoadPrimary)
            .map_err(|_| Error::other("clipboard worker is dead"))?;

        if let Ok(reply) = self.request_receiver.recv() {
            reply
        } else {
            // The clipboard thread is dead, however we shouldn't crash
            // downstream, so propogating an error.
            Err(std::io::Error::other("clipboard is dead."))
        }
    }

    /// Store to a primary clipboard.
    ///
    /// Stores to a primary clipboard on a last observed seat.
    pub fn store_primary<T: Into<String>>(&self, text: T) -> Result<()> {
        self.store_selection(text.into(), true)
    }

    fn store_selection(&self, text: String, primary: bool) -> Result<()> {
        let (sender, receiver) = mpsc::channel();
        let request = if primary {
            worker::Command::StorePrimary(text, sender)
        } else {
            worker::Command::Store(text, sender)
        };
        self.request_sender.send(request).map_err(|_| Error::other("clipboard worker is dead"))?;
        receiver
            .recv()
            .map_err(|_| Error::other("clipboard worker exited before accepting selection"))?
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // Shutdown smithay-clipboard.
        let _ = self.request_sender.send(worker::Command::Exit);
        if let Some(clipboard_thread) = self.clipboard_thread.take() {
            let _ = clipboard_thread.join();
        }
    }
}
