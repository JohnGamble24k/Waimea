//! Local remote control: newline-delimited JSON over a Unix domain socket
//! (`$XDG_RUNTIME_DIR/waimea.sock`, else the config dir; mode 0600) or the
//! Windows named pipe `\\.\pipe\waimea`. `WAIMEA_SOCKET` overrides the
//! endpoint. See [`protocol`] for the messages.

pub mod client;
pub mod daemon;
pub mod keys;
pub mod protocol;
pub mod screen;
mod server;
mod transport;

pub use server::IpcServer;
pub use transport::{connect, endpoint};
