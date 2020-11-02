//! Utilities for dynamic service configuration in [`crate::process`].

use futures::prelude::*;
use std::sync::Arc;
use tokio::sync::{mpsc, Notify};

/// Persistent configuration for running the API and coco peer services.
pub struct Config {
    /// Secret key for the coco peer.
    ///
    /// If this is `None` coco is not started.
    pub key: Option<coco::keys::SecretKey>,
    /// If set, we use a temporary directory for on-disk persistence.
    pub temp_dir: Option<tempfile::TempDir>,
}

/// Error returned when constructing a new configuration
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to create temporary directory
    #[error("Failed to create temporary directory")]
    TempDir(
        #[source]
        #[from]
        std::io::Error,
    ),
}

impl Config {
    /// Create a new default configuration.c:w
    ///
    /// If `test_mode` is `true` then `Config::temp_dir` is set for temporary on-disk persistence.
    fn new(test_mode: bool) -> Result<Self, Error> {
        let temp_dir = if test_mode {
            Some(tempfile::tempdir()?)
        } else {
            None
        };
        Ok(Self {
            key: None,
            temp_dir,
        })
    }
}

/// Manages changes to [`Config`].
pub struct Manager {
    /// Notifier to restart the services
    reload_notify: Arc<Notify>,
    /// Sender side of the [`Message`] channel
    message_sender: mpsc::Sender<Message>,
    /// Receiver side of the [`Message`] channel
    message_receiver: mpsc::Receiver<Message>,
    /// The current configuration of the services
    config: Config,
    /// If true we are running the service in test mode.
    test_mode: bool,
}

impl Manager {
    /// Create a new manager.
    ///
    /// If `test_mode` is `true` then `Config::temp_dir` is set for temporary on-disk persitence.
    pub fn new(test_mode: bool) -> Result<Self, Error> {
        let config = Config::new(test_mode)?;
        let (message_sender, message_receiver) = mpsc::channel(10);
        Ok(Self {
            reload_notify: Arc::new(Notify::new()),
            message_sender,
            message_receiver,
            config,
            test_mode,
        })
    }

    /// Get a handle to send updates to [`Manager`].
    pub fn handle(&self) -> Handle {
        Handle {
            reload_notify: self.reload_notify.clone(),
            message_sender: self.message_sender.clone(),
        }
    }

    /// Get the current configuration.
    pub fn config(&mut self) -> Result<&Config, Error> {
        while let Ok(message) = self.message_receiver.try_recv() {
            match message {
                Message::Reset => self.config = Config::new(self.test_mode)?,
                Message::SetSecretKey(key) => self.config.key = Some(key),
                Message::Seal => self.config.key = None,
            }
        }

        Ok(&self.config)
    }

    /// Returns a future that becomes ready when the service needs to restart because the
    /// configuration has changed.
    pub fn notified_restart(&mut self) -> impl Future<Output = ()> + Send + 'static {
        let reload_notify = Arc::new(Notify::new());
        self.reload_notify = reload_notify.clone();
        async move { reload_notify.notified().await }
    }
}

/// Messages that are sent from [`Handle`] to [`Manager`] to change the service configuration.
#[allow(clippy::clippy::large_enum_variant)]
enum Message {
    /// Reset the service to the initial configuration and delete all persisted state
    Reset,
    /// Unseal the key store with the given secret key
    SetSecretKey(coco::keys::SecretKey),
    /// Seal the key store and reload the services
    Seal,
}

/// A handle to communicate with [`Manager`].
#[derive(Clone)]
pub struct Handle {
    /// Notifier to restart the services
    reload_notify: Arc<Notify>,
    /// Sender side of the [`Message`] channel
    message_sender: mpsc::Sender<Message>,
}

impl Handle {
    /// Reset the service to the initial configuration and delete all persisted state
    pub fn reset(&mut self) {
        self.send_message(Message::Reset)
    }

    /// Unseal the key store with the given secret key
    pub fn set_secret_key(&mut self, key: coco::keys::SecretKey) {
        self.send_message(Message::SetSecretKey(key))
    }

    /// Seal the key store and reload the services
    pub fn seal(&mut self) {
        self.send_message(Message::Seal)
    }

    /// Send [`Message`] to [`Manager`]
    fn send_message(&mut self, message: Message) {
        #![allow(clippy::panic)]
        match self.message_sender.try_send(message) {
            Ok(()) => {},
            Err(err) => match err {
                mpsc::error::TrySendError::Full(_) => {
                    // In practice we can’t send more than one update message at a time.
                    panic!("service::Manager message queue is full")
                },
                mpsc::error::TrySendError::Closed(_) => {
                    // The manager must not be dropped before all handles are dropped.
                    panic!("service::Manager meesage queue is closed")
                },
            },
        }
        self.reload_notify.notify();
    }

    /// Create a handle where none of the methods have any effect.
    #[cfg(test)]
    pub fn dummy() -> Self {
        let (message_sender, mut message_receiver) = mpsc::channel(1);
        tokio::spawn(async move {
            loop {
                if message_receiver.recv().await.is_none() {
                    break;
                }
            }
        });
        Self {
            reload_notify: Arc::new(Notify::new()),
            message_sender,
        }
    }
}
