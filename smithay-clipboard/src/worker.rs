use std::io::{Error, Result};
use std::sync::mpsc::{Sender, SyncSender};

use sctk::reexports::calloop::channel::Channel;
use sctk::reexports::calloop::{channel, EventLoop};
use sctk::reexports::calloop_wayland_source::WaylandSource;
use sctk::reexports::client::globals::registry_queue_init;
use sctk::reexports::client::Connection;

use crate::state::{SelectionTarget, State};

/// Spawn a clipboard worker, which dispatches its own `EventQueue` and handles
/// clipboard requests.
pub fn spawn(
    name: String,
    display: Connection,
    rx_chan: Channel<Command>,
    worker_replier: Sender<Result<String>>,
    ready: SyncSender<Result<()>>,
) -> Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new().name(name).spawn(move || {
        if let Err(error) = worker_impl(display, rx_chan, worker_replier, &ready) {
            let _ = ready.send(Err(error));
        }
    })
}

/// Clipboard worker thread command.
pub enum Command {
    /// Store data to a clipboard.
    Store(String, Sender<Result<()>>),
    /// Store data to a primary selection.
    StorePrimary(String, Sender<Result<()>>),
    /// Load data from a clipboard.
    Load,
    /// Load primary selection.
    LoadPrimary,
    /// Shutdown the worker.
    Exit,
}

/// Handle clipboard requests.
fn worker_impl(
    connection: Connection,
    rx_chan: Channel<Command>,
    reply_tx: Sender<Result<String>>,
    ready: &SyncSender<Result<()>>,
) -> Result<()> {
    let (globals, event_queue) = registry_queue_init(&connection).map_err(|error| {
        Error::other(format!("clipboard registry initialization failed: {error}"))
    })?;
    let mut event_loop = EventLoop::<State>::try_new()
        .map_err(|error| Error::other(format!("clipboard event loop creation failed: {error}")))?;
    let loop_handle = event_loop.handle();
    let mut state = State::new(&globals, &event_queue.handle(), loop_handle.clone(), reply_tx)
        .ok_or_else(|| Error::other("compositor exposes no clipboard selection protocol"))?;

    loop_handle
        .insert_source(rx_chan, |event, _, state| {
            if let channel::Event::Msg(event) = event {
                match event {
                    Command::StorePrimary(contents, reply) => {
                        let _ =
                            reply.send(state.store_selection(SelectionTarget::Primary, contents));
                    },
                    Command::Store(contents, reply) => {
                        let _ =
                            reply.send(state.store_selection(SelectionTarget::Clipboard, contents));
                    },
                    Command::Load if state.data_device_manager_state.is_some() => {
                        if let Err(err) = state.load_selection(SelectionTarget::Clipboard) {
                            let _ = state.reply_tx.send(Err(err));
                        }
                    },
                    Command::LoadPrimary if state.primary_selection_manager_state.is_some() => {
                        if let Err(err) = state.load_selection(SelectionTarget::Primary) {
                            let _ = state.reply_tx.send(Err(err));
                        }
                    },
                    Command::Load | Command::LoadPrimary => {
                        let _ = state
                            .reply_tx
                            .send(Err(Error::other("requested selection is not supported")));
                    },
                    Command::Exit => state.exit = true,
                }
            }
        })
        .map_err(|error| {
            Error::other(format!("clipboard command source registration failed: {error}"))
        })?;
    WaylandSource::new(connection, event_queue).insert(loop_handle).map_err(|error| {
        Error::other(format!("clipboard event queue registration failed: {error}"))
    })?;
    ready
        .send(Ok(()))
        .map_err(|_| Error::other("clipboard owner dropped during initialization"))?;
    while !state.exit {
        event_loop
            .dispatch(None, &mut state)
            .map_err(|error| Error::other(format!("clipboard dispatch failed: {error}")))?;
    }
    Ok(())
}
